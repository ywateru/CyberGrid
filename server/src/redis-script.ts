/** One bounded transaction over individually indexed records, never a global state blob. */
export const REDIS_SCRIPT=String.raw`
local p=KEYS[1]
local op=ARGV[1]
local a=cjson.decode(ARGV[2])
local tm=redis.call('TIME');local now=tonumber(tm[1])*1000+math.floor(tonumber(tm[2])/1000)
local function key(t) return p..':'..t end
local function get(t,id) local v=redis.call('HGET',key(t),id);if v then return cjson.decode(v) end end
local function put(t,id,v,expiry) redis.call('HSET',key(t),id,cjson.encode(v));if expiry then redis.call('ZADD',key(t..':expiry'),expiry,id) end end
local function del(t,id) redis.call('HDEL',key(t),id);redis.call('ZREM',key(t..':expiry'),id) end
local function rows(t) local all=redis.call('HGETALL',key(t));local out={};for i=1,#all,2 do out[all[i]]=cjson.decode(all[i+1]) end;return out end
local function size(t) return redis.call('HLEN',key(t)) end
local function count(t,f) local n=0;for id,v in pairs(rows(t)) do if f(v,id) then n=n+1 end end;return n end
local function err(code,status) return cjson.encode({error=code,status=status or 409}) end
local function ok(v) return cjson.encode({value=v}) end
-- Detect wrong-type corruption before any business writes (Lua has no rollback).
for _,t in ipairs({'meta','sessions','devices','registrations','admissions','fences','signals','dedup','rates','waiters'}) do
 local ty=redis.call('TYPE',key(t)).ok;if ty~='none' and ty~='hash' then return err('COORDINATOR_UNAVAILABLE',503) end
 local ex=redis.call('TYPE',key(t..':expiry')).ok;if ex~='none' and ex~='zset' then return err('COORDINATOR_UNAVAILABLE',503) end
end
for _,t in ipairs({'initialized','revision','sequence','admission-sequence'}) do local ty=redis.call('TYPE',key(t)).ok;if ty~='none' and ty~='string' then return err('COORDINATOR_UNAVAILABLE',503) end end
local meta=get('meta','state')
if op=='init' then
 if not meta then
  if redis.call('EXISTS',key('initialized'))==1 then return err('COORDINATOR_UNAVAILABLE',503) end
  for _,t in ipairs({'meta','sessions','devices','registrations','admissions','fences','signals','dedup','rates','waiters'}) do
   if size(t)>0 or redis.call('ZCARD',key(t..':expiry'))>0 then return err('COORDINATOR_UNAVAILABLE',503) end
  end
  if redis.call('EXISTS',key('revision'),key('sequence'),key('admission-sequence'))>0 then return err('COORDINATOR_UNAVAILABLE',503) end
  meta={epoch=a.epoch,seed=a.seed,startedAt=now,limits=a.limits,registrationMode=a.registrationMode};put('meta','state',meta);redis.call('SET',key('initialized'),'1');if a.registrationMode=='finite-v2' then redis.call('SET',key('admission-sequence'),'0') end
 end
 if redis.call('GET',key('initialized'))~='1' or not meta.epoch or not meta.seed or not meta.startedAt or not meta.limits then return err('COORDINATOR_UNAVAILABLE',503) end
 if (meta.registrationMode or 'legacy-v1')~=a.registrationMode then return err('REGISTRATION_MODE_MISMATCH',503) end
 if a.registrationMode=='finite-v2' and not redis.call('GET',key('admission-sequence')) then return err('COORDINATOR_UNAVAILABLE',503) end
 if not meta.registrationMode then meta.registrationMode='legacy-v1';put('meta','state',meta) end
 meta.revision=tonumber(redis.call('GET',key('revision')) or '0');return ok(meta)
end
if not meta or meta.epoch~=a.epoch then return err('COORDINATOR_RESET') end
if meta.registrationMode~=a.registrationMode then return err('REGISTRATION_MODE_MISMATCH',503) end
local finite=meta.registrationMode=='finite-v2'
local L=meta.limits
local function rev() return tonumber(redis.call('GET',key('revision')) or '0') end
local function changed() redis.call('INCR',key('revision')) end
local function sweep(t)
 local ids=redis.call('ZRANGEBYSCORE',key(t..':expiry'),'-inf',now)
 for _,id in ipairs(ids) do del(t,id) end
 return #ids
end
local removed=sweep('sessions')+sweep('devices');sweep('signals');sweep('registrations');sweep('admissions');sweep('dedup');sweep('rates');sweep('waiters');if removed>0 then changed() end
local function live(s)
 if not s or s.expiresAt<=now then return false,'INVALID_SESSION' end
 if s.superseded then return false,'SESSION_SUPERSEDED' end
 if s.purpose=='device' then local d=get('devices',s.deviceId);if not d then return false,'INVALID_SESSION' end;if s.protocol and d.incarnation~=s.incarnation then return false,'SESSION_SUPERSEDED' end end
 return true
end
local function current(hash) local s=get('sessions',hash);local valid,e=live(s);if not valid then return nil,e end;return s end
local function online(d)
 if not d or d.lastSeen<=now-65000 then return false end
 local s=get('sessions',d.sessionHash);return live(s)
end
local function cap(t,global,user,peruser,replacing)
 local n=count(t,function(v,id) return id~=replacing end)
 if n>=global then return 'CAPACITY_EXCEEDED',503 end
 if user and count(t,function(v,id) return id~=replacing and v.userId==user end)>=peruser then return 'CAPACITY_EXCEEDED',429 end
end
local function removeSignals(device)
 for id,v in pairs(rows('signals')) do if v.fromDeviceId==device or v.toDeviceId==device then del('signals',id) end end
end
-- Only finite fences have a proof that every issued ticket has expired.
if finite then
 local held={};for _,s in pairs(rows('sessions')) do if s.purpose=='device' and s.expiresAt>now and not s.superseded then held[s.deviceId]=true end end
 for id,f in pairs(rows('fences')) do
  if f.admissionVersion==2 and now>=f.maxIssuedNotAfter and not held[id] then del('fences',id) end
 end
end
if op=='admissionChallenge' then
 if not finite then return err('REGISTRATION_ADMISSION_INVALID') end
 local f=get('fences',a.logical);return ok({now=now,epoch=meta.epoch,domain=f and f.latestDomain or '0'})
end
if op=='admissionMint' then
 if not finite or a.claim.epoch~=meta.epoch then return err('REGISTRATION_ADMISSION_INVALID') end
 local old=get('admissions',a.id)
 if old then
  if old.content~=a.content then return err('REGISTRATION_ADMISSION_CONFLICT') end
  if now>=old.notAfter then return err('REGISTRATION_ADMISSION_EXPIRED') end
  return ok({epoch=meta.epoch,domain=old.domain,issuedAt=old.issuedAt,notAfter=old.notAfter})
 end
 if now>=a.claim.notAfter then return err('REGISTRATION_ADMISSION_EXPIRED') end
 local f=get('fences',a.logical)
 if (f and f.latestDomain or '0')~=a.claim.domain then return err('REGISTRATION_ADMISSION_CONFLICT') end
 if not f then local e,st=cap('fences',L.registrationKeys,a.userId,L.registrationKeysPerUser);if e then return err(e,st) end end
 if size('registrations')+size('admissions')+size('dedup')>=L.idempotency then return err('CAPACITY_EXCEEDED',503) end
 local order=tonumber(redis.call('GET',key('admission-sequence')))
 if not order or order<0 or order~=math.floor(order) or order>=9007199254740991 then return err('CAPACITY_EXCEEDED',503) end
 -- All rejecting checks precede the first mint write.
 order=redis.call('INCR',key('admission-sequence'));local domain=string.format('%.0f',order);local notAfter=now+86400000
 f=f or {userId=a.userId};f.admissionVersion=2;f.latestDomain=domain;f.maxIssuedNotAfter=math.max(f.maxIssuedNotAfter or 0,notAfter);f.revision=0;f.requestId='';f.content=''
 put('fences',a.logical,f);put('admissions',a.id,{content=a.content,domain=domain,issuedAt=now,notAfter=notAfter},notAfter)
 return ok({epoch=meta.epoch,domain=domain,issuedAt=now,notAfter=notAfter})
end
if op=='cleanup' then return ok(true) end
if op=='meta' then return ok({epoch=meta.epoch,revision=rev(),now=now,startedAt=meta.startedAt}) end
if op=='current' then local s,e=current(a.hash);if not s then return err(e,401) end;return ok(s) end
if op=='session' then return ok(get('sessions',a.hash) or cjson.null) end
if op=='register' then
 if a.expectedEpoch~=meta.epoch then return err('COORDINATOR_RESET') end
 local f=a.logical and get('fences',a.logical)
 if finite and a.logical then
  if not a.admission then return err('REGISTRATION_ADMISSION_REQUIRED') end
  if a.admission.epoch~=meta.epoch then return err('REGISTRATION_ADMISSION_INVALID') end
  if now>=a.admission.notAfter then return err('REGISTRATION_ADMISSION_EXPIRED') end
  if not f or f.admissionVersion~=2 or f.latestDomain~=a.admission.domain then return err('REGISTRATION_ADMISSION_CONFLICT') end
 end
 if f and a.registrationRevision<f.revision then return err('SESSION_SUPERSEDED') end
 local old=get('registrations',a.id)
 -- Recover only an exact pre-binding dynamic receipt; never create/migrate one.
 local legacy=a.legacy and a.binding and a.binding.policyMode=='dynamic' and (not a.logical or a.binding.authClientId==a.appId)
 local legacyFence=legacy and (not a.logical or (f and f.userId==a.userId and f.revision==a.registrationRevision and f.requestId==a.requestId and f.content==a.legacy.content))
 if legacy and legacyFence then
  if old and old.content==a.legacy.content and old.hash==a.legacy.hash then
   local s=current(old.hash);if not s then return err('REGISTRATION_ATTEMPT_EXPIRED') end
   if s.authClientId==nil and s.policyMode==nil and s.protocol=='memory-v1' and s.hash==a.legacy.hash and s.userId==a.userId and s.appId==a.appId and ((a.logical and s.purpose=='device' and s.deviceId==a.logical) or (not a.logical and s.purpose=='console' and s.deviceId==cjson.null)) then return ok(s) end
  elseif not old and a.logical then return err('REGISTRATION_ATTEMPT_EXPIRED') end
 end
 if f and a.registrationRevision==f.revision and (a.content~=f.content or a.requestId~=f.requestId) then return err('REGISTRATION_ID_CONFLICT') end
 if old then
  if old.content~=a.content then return err('REGISTRATION_ID_CONFLICT') end
  local s=current(old.hash);if not s then return err('REGISTRATION_ATTEMPT_EXPIRED') end
  return ok(s)
 end
 if f and a.registrationRevision==f.revision then return err('REGISTRATION_ATTEMPT_EXPIRED') end
 if a.logical and not f then local e,st=cap('fences',L.registrationKeys,a.userId,L.registrationKeysPerUser);if e then return err(e,st) end end
 local previous=a.logical and get('devices',a.logical)
 local replacing=previous and previous.sessionHash
 local e,st=cap('sessions',L.sessions,a.userId,L.sessionsPerUser,replacing);if e then return err(e,st) end
 if a.logical and not previous then e,st=cap('devices',L.devices,a.userId,L.devicesPerUser);if e then return err(e,st) end end
 local reclaim=0;for id,r in pairs(rows('registrations')) do if not finite and a.logical and r.deviceId==a.logical then reclaim=reclaim+1 end end
 local dedupReclaim=0;for _,r in pairs(rows('dedup')) do if replacing and r.sessionHash==replacing then dedupReclaim=dedupReclaim+1 end end
 if size('registrations')+size('admissions')+size('dedup')-reclaim-dedupReclaim>=L.idempotency then return err('CAPACITY_EXCEEDED',503) end
 local s={purpose=a.logical and 'device' or 'console',hash=a.hash,userId=a.userId,appId=a.appId,deviceId=a.logical or cjson.null,expiresAt=now+86400000,protocol='memory-v1'}
 if finite and a.logical then s.expiresAt=math.min(s.expiresAt,a.admission.notAfter);s.admissionDomain=a.admission.domain;s.admissionNotAfter=a.admission.notAfter end
 if a.binding then s.authClientId=a.binding.authClientId;s.policyMode=a.binding.policyMode end
 if a.logical then
  s.incarnation=a.incarnation
  if replacing then del('sessions',replacing) end
  for id,r in pairs(rows('registrations')) do if not finite and r.deviceId==a.logical then del('registrations',id) end end
  for id,r in pairs(rows('dedup')) do if r.sessionHash==replacing then del('dedup',id) end end
  put('devices',a.logical,{id=a.logical,userId=a.userId,appId=a.appId,name=a.name,lastSeen=now,ports=cjson.decode('[]'),tunnelUrl=cjson.null,certificateFingerprint=cjson.null,iroh=cjson.null,incarnation=s.incarnation,publishRevision=0,sessionHash=a.hash,snapshotDigest=''},s.expiresAt)
  if finite then f.revision=a.registrationRevision;f.requestId=a.requestId;f.content=a.content;put('fences',a.logical,f) else put('fences',a.logical,{userId=a.userId,revision=a.registrationRevision,requestId=a.requestId,content=a.content}) end
  removeSignals(a.logical)
 end
 put('sessions',a.hash,s,s.expiresAt);put('registrations',a.id,{content=a.content,hash=a.hash,deviceId=a.logical or cjson.null},s.expiresAt);changed();return ok(s)
end
if op=='revoke' then
 local s=get('sessions',a.hash);del('sessions',a.hash)
 if s and s.purpose=='device' then local d=get('devices',s.deviceId);if d and d.sessionHash==s.hash then del('devices',s.deviceId);removeSignals(s.deviceId) end end
 changed();return ok(true)
end
if op=='rate' then
 local r=get('rates',a.id)
 if not r then if size('rates')>=L.rateKeys then return err('CAPACITY_EXCEEDED',503) end;r={count=0,start=now} end
 r.count=r.count+1;put('rates',a.id,r,r.start+60000);return ok(r.count<=a.limit)
end
if a.sessionHash then local s,e=current(a.sessionHash);if not s then return err(e,401) end end
if op=='publish' then
 local s,e=current(a.hash);if not s then return err(e,401) end
 local d=get('devices',s.deviceId)
 local currentRevision=tonumber(d.publishRevisionExact or d.publishRevision)
 a.revision=tonumber(a.revisionText)
 if a.incarnation~=s.incarnation then return err('INCARNATION_MISMATCH') end
 if a.revision<currentRevision then return err('STALE_PUBLISH_REVISION') end
 if a.revision==currentRevision and a.digest~=d.snapshotDigest then return err('PUBLISH_REVISION_CONFLICT') end
 local history=d.bridgeHistory or {};local nextHistory={};local count=0
 for _,b in ipairs(history) do nextHistory[b.id]=b;count=count+1 end
 if a.revision~=currentRevision then
  for _,b in ipairs(a.bridgeRecords) do
   local old=nextHistory[b.id]
   if b.expires<=now or b.expires>now+300000 or (old and (tonumber(b.revision)<tonumber(old.revision) or (tonumber(b.revision)==tonumber(old.revision) and (b.identity~=old.identity or b.expires<old.expires)))) then return err('INVALID_INPUT',400) end
   if not old then count=count+1 end;if count>4 then return err('INVALID_INPUT',400) end;nextHistory[b.id]=b
  end
 end
 local storedHistory={};for _,b in pairs(nextHistory) do table.insert(storedHistory,b) end
 local change=not online(d) or a.digest~=d.snapshotDigest
 d.bridgeHistory=storedHistory;d.bridges=nil;d.bridgesJson=a.bridgesJson;d.ports=a.update.ports;d.tunnelUrl=a.update.tunnelUrl;d.certificateFingerprint=a.update.certificateFingerprint;d.iroh=a.update.iroh;d.publishRevision=a.revision;d.publishRevisionExact=a.revisionText;d.snapshotDigest=a.digest;d.lastSeen=now
 put('devices',d.id,d,math.min(now+86400000,s.admissionNotAfter or now+86400000));if change then changed() end;return ok(true)
end
if op=='directorySample' then
 local s,e=current(a.hash);if not s then return err(e,401) end
 if s.purpose=='device' and s.appId~=a.productId then return err('FORBIDDEN',403) end
 local out={};for _,d in pairs(rows('devices')) do if d.userId==s.userId and d.appId==a.productId then d.online=not not online(d);table.insert(out,d) end end
 return ok({epoch=meta.epoch,now=now,userId=s.userId,purpose=s.purpose,devices=out})
end
if op=='device' then local d=get('devices',a.id);if not d then return ok(cjson.null) end;d.online=not not online(d);return ok(d) end
if op=='devices' then
 local out={};for _,d in pairs(rows('devices')) do if d.userId==a.userId and d.appId==a.appId then d.online=not not online(d);table.insert(out,d) end end
 table.sort(out,function(x,y) return x.id<y.id end);return ok(out)
end
local function deliver(m,to,after)
 local from=get('devices',m.fromDeviceId);local target=get('devices',to)
 return m.toDeviceId==to and m.id>after and m.expiresAt>now and from and target and online(from) and (not m.fromIncarnation or m.fromIncarnation==from.incarnation) and (not m.toIncarnation or m.toIncarnation==target.incarnation)
end
if op=='postSignal' then
 local s,e=current(a.hash);if not s then return err(e,401) end
 local from=get('devices',s.deviceId);local to=get('devices',a.value.toDeviceId)
 if not from or not to or to.id==from.id or to.userId~=s.userId or to.appId~=s.appId or not online(from) or not online(to) then return err('PEER_UNAVAILABLE',404) end
 if a.value.toIncarnation~=to.incarnation then return err('INCARNATION_MISMATCH') end
 local old=get('dedup',a.id)
 if old then if old.content~=a.content then return err('SIGNAL_ID_CONFLICT') end;return ok(old.id) end
 if size('registrations')+size('admissions')+size('dedup')>=L.idempotency then return err('CAPACITY_EXCEEDED',503) end
 local n=0;local bytes=0;local inbox=0;local inboxBytes=0;local un=0;local ub=0
 for _,m in pairs(rows('signals')) do
  local b=string.len(m.payload);n=n+1;bytes=bytes+b
  if m.toDeviceId==to.id then inbox=inbox+1;inboxBytes=inboxBytes+b end
  local target=get('devices',m.toDeviceId);if target and target.userId==s.userId then un=un+1;ub=ub+b end
 end
 local b=string.len(a.value.payload)
 if n>=L.signals or bytes+b>L.signalBytes then return err('CAPACITY_EXCEEDED',503) end
 if inbox>=L.signalsPerDevice or inboxBytes+b>L.signalBytesPerDevice or un>=L.signalsPerUser or ub+b>L.signalBytesPerUser then return err('CAPACITY_EXCEEDED',429) end
 local id=redis.call('INCR',key('sequence'));local m=a.value;m.id=id;m.fromDeviceId=s.deviceId;m.fromIncarnation=s.incarnation;m.expiresAt=now+120000
 put('signals',tostring(id),m,m.expiresAt);put('dedup',a.id,{id=id,content=a.content,sessionHash=s.hash},m.expiresAt);return ok(id)
end
if op=='signals' then
 local out={};for _,m in pairs(rows('signals')) do if deliver(m,a.id,a.after) then table.insert(out,m) end end
 table.sort(out,function(x,y) return x.id<y.id end);local batch={};local bytes=0
 for _,m in ipairs(out) do m.coordinatorEpoch=meta.epoch;m.expiresAt=nil;m.toDeviceId=nil;local b=string.len(cjson.encode(m));if #batch>=100 or bytes+b>1048576 then break end;table.insert(batch,m);bytes=bytes+b end
 return ok(batch)
end
if op=='waitAcquire' then
 local s,e=current(a.hash);if not s then return err(e,401) end
 if size('waiters')>=L.waiters then return err('CAPACITY_EXCEEDED',503) end
 if count('waiters',function(w) return w.deviceId==s.deviceId end)>=L.waitersPerDevice then return err('CAPACITY_EXCEEDED',429) end
 put('waiters',a.id,{deviceId=s.deviceId,owner=a.id},now+17000);return ok(true)
end
if op=='waitRelease' then local w=get('waiters',a.id);if w and w.owner==a.id then del('waiters',a.id) end;return ok(true) end
if op=='admin' then
 local active={};local sessions=0;for _,s in pairs(rows('sessions')) do if live(s) then sessions=sessions+1;if s.purpose=='device' then active[s.deviceId]=true end end end
 local all={};local totals={sessions=sessions,devices=0,onlineDevices=0,offlineDevices=0,ports=0,queuedSignals=0,waiters=size('waiters')}
 for _,d in pairs(rows('devices')) do if active[d.id] then
  local on=not not online(d);totals.devices=totals.devices+1;if on then totals.onlineDevices=totals.onlineDevices+1 else totals.offlineDevices=totals.offlineDevices+1 end
  local ps={};for _,port in ipairs(d.ports) do table.insert(ps,{port=port.port,type=port.type});totals.ports=totals.ports+1 end
  if (not a.query.appId or d.appId==a.query.appId) and (a.query.online==nil or on==a.query.online) then table.insert(all,{id=d.id,appId=d.appId,name=d.name,online=on,lastSeen=d.lastSeen,ports=ps,hasTunnel=d.tunnelUrl~=cjson.null}) end
 end end
 for _,m in pairs(rows('signals')) do if active[m.toDeviceId] and deliver(m,m.toDeviceId,0) then totals.queuedSignals=totals.queuedSignals+1 end end
 table.sort(all,function(x,y) return x.id<y.id end);local page={};local offset=(a.query.page-1)*a.query.pageSize;for i=offset+1,math.min(#all,offset+a.query.pageSize) do table.insert(page,all[i]) end
 return ok({now=now,startedAt=meta.startedAt,epoch=meta.epoch,totals=totals,devices=page,total=#all})
end
return err('COORDINATOR_UNAVAILABLE',503)
`;
