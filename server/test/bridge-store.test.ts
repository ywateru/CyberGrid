import {test} from 'node:test';
import assert from 'node:assert/strict';
import {randomBytes,randomUUID} from 'node:crypto';
import {MemoryStore} from '../src/memory.js';
import {RedisStore,redisTransport} from '../src/redis.js';
import {redisFixture} from './redis-fixture.js';
import type {BrowserBridge,DeviceSession,Store} from '../src/types.js';
const wait=(ms:number)=>new Promise(r=>setTimeout(r,ms));
const makeBridge=(expiresAt:number,id='bridge'):BrowserBridge=>({id,protocol:'noise.ik.v1',providerId:'quick-tunnel',url:'wss://fixture.trycloudflare.com/bridge',installationId:'installation',publicKey:randomBytes(32).toString('base64url'),addressRevision:1,expiresAt:new Date(expiresAt).toISOString(),limits:{maxFrameBytes:4096,maxStreams:4,maxPendingRequests:8}});
const publication=(bs:BrowserBridge[]=[])=>({ports:[],tunnelUrl:null,certificateFingerprint:'a'.repeat(64),iroh:null,bridges:bs});
for(const kind of ['memory','redis'] as const)test(`${kind}: bounded bridge history survives withdrawal, exact retry and incarnation reset`,async t=>{
 let now=Date.now();const f=kind==='redis'?await redisFixture(t):undefined;if(kind==='redis'&&!f)return;const store:Store=f?f.store():new MemoryStore({},'legacy-v1',()=>now);if(!f)t.after(()=>store.close());await store.ready?.();const key=randomBytes(32).toString('base64url'),requestId=randomUUID();const r=await store.register('owner','product','fixture',key,requestId,now,1),s=r.session as DeviceSession;let rev=1;const b=makeBridge(now+200000);
 const publish=(bs:BrowserBridge[],revision=rev)=>store.publish(s,publication(bs),s.incarnation,revision,0);await publish([b]);await publish([b]);rev++;await publish([]);rev++;
 await assert.rejects(publish([{...b,url:'wss://fixture.trycloudflare.com/changed'}]),{code:'INVALID_INPUT'});await assert.rejects(publish([{...b,limits:{...b.limits,maxStreams:3}}]),{code:'INVALID_INPUT'});await assert.rejects(publish([{...b,expiresAt:new Date(Date.parse(b.expiresAt)-1).toISOString()}]),{code:'INVALID_INPUT'});await assert.rejects(publish([{...b,addressRevision:0}]),{code:'INVALID_INPUT'});
 await publish([{...b,expiresAt:new Date(Date.parse(b.expiresAt)+1).toISOString()}]);rev++;await publish([{...b,url:'wss://fixture.trycloudflare.com/changed',addressRevision:2}]);rev++;
 for(const id of ['two','three','four']){await publish([makeBridge(now+180000,id)]);rev++;await publish([]);rev++;}const replay=await store.register('owner','product','fixture',key,requestId,now,1);assert.equal(replay.session.incarnation,s.incarnation);await assert.rejects(publish([makeBridge(now+180000,'five')]),{code:'INVALID_INPUT'});
 // Replay of registration is not a new incarnation and cannot erase historical IDs.
 const next=await store.register('owner','product','fixture',key,randomUUID(),now,2);assert.notEqual(next.session.incarnation,s.incarnation);await store.publish(next.session as DeviceSession,publication([makeBridge(now+180000,'five')]),next.session.incarnation,1,0);
 const ds=await store.sampleDirectory(next.session,'product','policy',now);assert.equal(ds.devices[0]!.bridges![0]!.id,'five');assert.ok(!JSON.stringify(ds).includes('bridgeHistory'));assert.ok(!JSON.stringify(ds).includes('snapshotDigest'));
});
test('MemoryStore bridge deadline uses injected clock, retry after expiry never extends address',async t=>{
 let now=Date.now();const store=new MemoryStore({},'legacy-v1',()=>now);t.after(()=>store.close());const r=await store.register('owner','product','fixture',randomBytes(32).toString('base64url'),randomUUID(),now,1),s=r.session as DeviceSession,b=makeBridge(now+100);await store.publish(s,publication([b]),s.incarnation,1,0);now+=101;
 await store.publish(s,publication([b]),s.incarnation,1,0);assert.equal((await store.sampleDirectory(s,'product','policy',0)).devices[0]!.bridges![0]!.expiresAt,b.expiresAt);await assert.rejects(store.publish(s,publication([b]),s.incarnation,2,0),{code:'INVALID_INPUT'});
 await assert.rejects(store.publish(s,publication([makeBridge(now+300001)]),s.incarnation,2,0),{code:'INVALID_INPUT'});
});
test('Redis TIME checks at transaction commit, same revision expired replay acknowledges without extending expiry',async t=>{
 const f=await redisFixture(t);if(!f)return;const base=redisTransport({REDIS_URL:f.url});let delay=0;const store=new RedisStore({eval:async(script,key,args)=>{if(args[0]==='publish'&&delay)await wait(delay);return base.eval(script,key,args);},close:()=>base.close()},f.prefix);t.after(()=>store.close());await store.ready();const r=await store.register('owner','product','fixture',randomBytes(32).toString('base64url'),randomUUID(),0,1),s=r.session as DeviceSession;
 const b=makeBridge(Date.now()+50);delay=100;await assert.rejects(store.publish(s,publication([b]),s.incarnation,1,Date.now()),{code:'INVALID_INPUT'});assert.equal((await store.device(s.deviceId,0))!.publishRevision,0);
 delay=0;const accepted=makeBridge(Date.now()+100);await store.publish(s,publication([accepted]),s.incarnation,1,0);await wait(110);await store.publish(s,publication([accepted]),s.incarnation,1,0);const d=(await store.device(s.deviceId,0))!;assert.equal(d.bridges![0]!.expiresAt,accepted.expiresAt);assert.equal(d.publishRevision,1);await assert.rejects(store.publish(s,publication([accepted]),s.incarnation,2,0),{code:'INVALID_INPUT'});
});
test('bridge publication bounds cover complete 24KiB snapshot and rollback leaves prior revision intact',async t=>{
 const f=await redisFixture(t);if(!f)return;const store=f.store();await store.ready();const r=await store.register('owner','product','fixture',randomBytes(32).toString('base64url'),randomUUID(),0,1),s=r.session as DeviceSession;await store.publish(s,publication(),s.incarnation,1,0);
 await assert.rejects(store.publish(s,{...publication([makeBridge(Date.now()+60000)]),ports:Array.from({length:32},(_,n)=>({id:String(n),port:1234,type:'x'.repeat(1000),transport:'tcp' as const}))},s.incarnation,2,0),{code:'DEVICE_METADATA_TOO_LARGE'});assert.equal((await store.device(s.deviceId,0))!.publishRevision,1);
});

test('Redis bridge safe integer address revisions survive cjson persistence and exact replay',async t=>{
 const f=await redisFixture(t);if(!f)return;const store=f.store();await store.ready();const r=await store.register('owner','product','fixture',randomBytes(32).toString('base64url'),randomUUID(),0,1),s=r.session as DeviceSession;const b={...makeBridge(Date.now()+200000),addressRevision:Number.MAX_SAFE_INTEGER};await store.publish(s,publication([b]),s.incarnation,1,0);assert.equal((await store.device(s.deviceId,0))!.bridges![0]!.addressRevision,Number.MAX_SAFE_INTEGER);await store.publish(s,publication([b]),s.incarnation,1,0);
 await assert.rejects(store.publish(s,publication([{...b,addressRevision:Number.MAX_SAFE_INTEGER-1}]),s.incarnation,2,0),{code:'INVALID_INPUT'});await assert.rejects(store.publish(s,publication([{...b,publicKey:randomBytes(32).toString('base64url')}]),s.incarnation,2,0),{code:'INVALID_INPUT'});assert.ok(!JSON.stringify(await store.sampleDirectory(s,'product','policy',0)).includes('bridgesJson'));
});

test('Redis safe publication revisions preserve MAX_SAFE_INTEGER ordering and bound withdrawn bridge identities',async t=>{
 const f=await redisFixture(t);if(!f)return;const store=f.store();await store.ready();const r=await store.register('owner','product','fixture',randomBytes(32).toString('base64url'),randomUUID(),0,1),s=r.session as DeviceSession;const b={...makeBridge(Date.now()+200000),addressRevision:Number.MAX_SAFE_INTEGER-1};
 await store.publish(s,publication([b]),s.incarnation,Number.MAX_SAFE_INTEGER-2,0);await store.publish(s,publication(),s.incarnation,Number.MAX_SAFE_INTEGER-1,0);assert.equal((await store.device(s.deviceId,0))!.publishRevision,Number.MAX_SAFE_INTEGER-1);
 await assert.rejects(store.publish(s,publication([{...b,url:'wss://fixture.trycloudflare.com/changed'}]),s.incarnation,Number.MAX_SAFE_INTEGER,0),{code:'INVALID_INPUT'});const next={...b,addressRevision:Number.MAX_SAFE_INTEGER,url:'wss://fixture.trycloudflare.com/changed'};await store.publish(s,publication([next]),s.incarnation,Number.MAX_SAFE_INTEGER,0);await store.publish(s,publication([next]),s.incarnation,Number.MAX_SAFE_INTEGER,0);
 const d=(await store.device(s.deviceId,0))!;assert.equal(d.publishRevision,Number.MAX_SAFE_INTEGER);assert.equal(d.bridges![0]!.addressRevision,Number.MAX_SAFE_INTEGER);await assert.rejects(store.publish(s,publication(),s.incarnation,Number.MAX_SAFE_INTEGER-1,0),{code:'STALE_PUBLISH_REVISION'});await assert.rejects(store.publish(s,publication([{...next,limits:{...next.limits,maxStreams:1}}]),s.incarnation,Number.MAX_SAFE_INTEGER,0),{code:'PUBLISH_REVISION_CONFLICT'});
 const sample=await store.sampleDirectory(s,'product','policy',0);assert.equal(sample.devices[0]!.publishRevision,Number.MAX_SAFE_INTEGER);for(const field of ['bridgesJson','publishRevisionExact','bridgeHistory','snapshotDigest'])assert.ok(!JSON.stringify(sample).includes(field));
});

test('Memory directory sampling captures one authoritative clock value for lease and timestamp',async t=>{
 let time=Date.now(),reads=0;const store=new MemoryStore({},'legacy-v1',()=>{reads++;return time;});t.after(()=>store.close());const r=await store.register('owner','product','fixture',randomBytes(32).toString('base64url'),randomUUID(),time,1);time+=65001;reads=0;const sample=await store.sampleDirectory(r.session,'product','policy',0);assert.equal(reads,1);assert.equal(sample.sampledAt,time);assert.equal(sample.devices[0]!.online,false);
});
