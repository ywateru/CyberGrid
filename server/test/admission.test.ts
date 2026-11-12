import {test} from 'node:test';
import assert from 'node:assert/strict';
import {createServer} from 'node:http';
import {randomBytes,randomUUID} from 'node:crypto';
import {inspect} from 'node:util';
import {generateKeyPair,exportJWK,createLocalJWKSet,SignJWT} from 'jose';
import {RedisStore,redisTransport,type RedisTransport} from '../src/redis.js';
import {MemoryStore,type MemoryLimits} from '../src/memory.js';
import {registrationMode,keyHash,signAdmission,verifyAdmission} from '../src/admission.js';
import {ApiError,type DeviceSession} from '../src/types.js';
import {LogtoIdentity} from '../src/identity.js';
import {coordinatorAuthConfiguration} from '../src/deployment-config.js';
import {ProductPolicy} from '../src/products.js';
import {createService} from '../src/service.js';
import {serveAPI,type Handler} from '../src/http.js';
import {redisFixture} from './redis-fixture.js';
const A='NativeCaseA',B='NativeCaseB',C='OtherNative',SPA='ConsoleSPA',user='fixture-owner';
const key=()=>randomBytes(32).toString('base64url');
const code=(expected:string)=>(error:unknown)=>error instanceof ApiError&&error.code===expected;
const binding={authClientId:A,policyMode:'static' as const};
const snapshot={ports:[{id:'echo',port:6789,type:'echo',transport:'tcp' as const}],certificateFingerprint:'a'.repeat(64),tunnelUrl:null};
const TIME="local tm=redis.call('TIME');local now=tonumber(tm[1])*1000+math.floor(tonumber(tm[2])/1000)";
async function fixture(t:any,limits:Partial<MemoryLimits>={}){
 const redis=await redisFixture(t);if(!redis)return;const clock={now:Date.now()},stores:RedisStore[]=[];
 // Test-only script variant: actual Redis executes every atomic command. There is
 // no client-controlled time argument or clock branch in production code.
 const store=(mode:'finite-v2'|'legacy-v1'='finite-v2',prefix=redis.prefix,custom=limits)=>{const transport=redisTransport({REDIS_URL:redis.url}),wrapper:RedisTransport={eval(script,k,args){assert.ok(script.includes(TIME));return transport.eval(script.replace(TIME,'local now='+clock.now),k,args);},close:()=>transport.close()},s=new RedisStore(wrapper,prefix,custom,mode);stores.push(s);return s;};
 t.after(()=>stores.forEach(s=>s.close()));const a=store(),b=store();await Promise.all([a.ready(),b.ready()]);return {redis,clock,store,a,b};
}
async function ticket(store:RedisStore|MemoryStore,k=key(),owner=user,client=A,product=A){const challenge=await store.challenge(owner,product,client,k),requestId=randomUUID(),receipt=await store.admit(owner,product,client,k,challenge.challenge,requestId);return {...receipt,k,challenge,requestId,owner,client,product};}
async function register(store:RedisStore|MemoryStore,t:Awaited<ReturnType<typeof ticket>>,revision=1,requestId=randomUUID()){return store.register(t.owner,t.product,'fixture',t.k,requestId,Date.now(),revision,store.epoch,{authClientId:t.client,policyMode:'static'},t.admissionTicket);}
async function http(t:any,handle:Handler){const server=createServer((req,res)=>void serveAPI(req,res,handle));await new Promise<void>(resolve=>server.listen(0,'127.0.0.1',resolve));t.after(()=>new Promise<void>(resolve=>server.close(()=>resolve())));return 'http://127.0.0.1:'+(server.address() as {port:number}).port;}
test('admission codec requires canonical bounded base64url, exact fields and separated HMAC domains',()=>{
 assert.equal(registrationMode(undefined),'legacy-v1');for(const invalid of ['', 'finite',' finite-v2','FINITE-V2'])assert.throws(()=>registrationMode(invalid));
 const seed=randomBytes(32),context={userId:user,productId:A,authClientId:A,keyHash:keyHash(key())},claim={...context,epoch:randomUUID(),domain:'0',nonce:randomUUID(),issuedAt:1000,notAfter:121000},raw=signAdmission(seed,'challenge',claim);assert.deepEqual(verifyAdmission(seed,'challenge',raw,context,claim.epoch),claim);
 assert.throws(()=>verifyAdmission(seed,'challenge',raw,context,randomUUID()),code('REGISTRATION_ADMISSION_INVALID'));
 const [body,mac]=raw.split('.') as [string,string];for(const invalid of [raw+'=',body+'=.'+mac,body+'.'+mac.slice(1),body+'.'+Buffer.alloc(32).toString('base64url'),raw+'.extra','x'.repeat(4097),undefined])assert.throws(()=>verifyAdmission(seed,'challenge',invalid,context,claim.epoch),code('REGISTRATION_ADMISSION_INVALID'));
 assert.throws(()=>verifyAdmission(seed,'ticket',raw,context,claim.epoch),code('REGISTRATION_ADMISSION_INVALID'));for(const changes of [{userId:'other'},{productId:C},{authClientId:B},{keyHash:'a'.repeat(64)}])assert.throws(()=>verifyAdmission(seed,'challenge',raw,{...context,...changes},claim.epoch),code('REGISTRATION_ADMISSION_INVALID'));
 for(const changed of [{domain:'00'},{domain:'9007199254740992'},{notAfter:121001},{nonce:'bad'}])assert.throws(()=>verifyAdmission(seed,'challenge',signAdmission(seed,'challenge',{...claim,...changed}),context,claim.epoch),code('REGISTRATION_ADMISSION_INVALID'));
 const marker='credential-material-marker';try{verifyAdmission(seed,'ticket',marker,context,claim.epoch);assert.fail();}catch(error){assert.ok(!inspect(error).includes(marker));assert.ok(!JSON.stringify(error).includes(marker));}
});
test('real Redis mint is idempotent after challenge expiry, CAS fenced and retains old active until new registration',async t=>{
 const f=await fixture(t);if(!f)return;const {a,b,clock}=f,first=await ticket(a),requestId=randomUUID(),active=await register(a,first,20,requestId);await a.publish(active.session as DeviceSession,snapshot,active.session.incarnation,1,0);
 assert.equal((await b.register(user,A,'fixture',first.k,requestId,0,20,b.epoch,binding,first.admissionTicket)).sessionKey,active.sessionKey);
 clock.now+=120001;assert.equal((await b.admit(user,A,A,first.k,first.challenge.challenge,first.requestId)).admissionTicket,first.admissionTicket);
 await assert.rejects(b.admit(user,A,A,first.k,first.challenge.challenge,randomUUID()),code('REGISTRATION_ADMISSION_EXPIRED'));
 const c1=await a.challenge(user,A,A,first.k),c2=await b.challenge(user,A,A,first.k),results=await Promise.allSettled([a.admit(user,A,A,first.k,c1.challenge,randomUUID()),b.admit(user,A,A,first.k,c2.challenge,randomUUID())]);assert.equal(results.filter(r=>r.status==='fulfilled').length,1);assert.equal(results.filter(r=>r.status==='rejected'&&code('REGISTRATION_ADMISSION_CONFLICT')(r.reason)).length,1);
 await a.current(active.session,0);assert.equal((await a.device(active.session.deviceId!,0))!.id,active.session.deviceId);
 await assert.rejects(register(b,first,999),code('REGISTRATION_ADMISSION_CONFLICT'));
 const second=(results.find(r=>r.status==='fulfilled') as PromiseFulfilledResult<any>).value,next=await register(b,{...first,...second},1);assert.equal(next.session.deviceId,active.session.deviceId);assert.notEqual(next.session.incarnation,active.session.incarnation);assert.notEqual(next.sessionKey,active.sessionKey);await a.revoke(active.session.hash);await b.current(next.session,0);
 await assert.rejects(a.register(user,A,'fixture',first.k,requestId,0,20,a.epoch,binding,second.admissionTicket),code('REGISTRATION_ID_CONFLICT'));
 const sameChallengeNewId=randomUUID(),challenge=await a.challenge(user,A,A,first.k);await a.admit(user,A,A,first.k,challenge.challenge,sameChallengeNewId);const modified=await a.challenge(user,A,A,first.k);await assert.rejects(a.admit(user,A,A,first.k,modified.challenge,sameChallengeNewId),code('REGISTRATION_ADMISSION_CONFLICT'));
});
test('real Redis 24h GC rejects old zero/nonzero challenges and tickets, clamps heartbeat and never reuses domain',async t=>{
 const f=await fixture(t);if(!f)return;const {a,b,clock}=f,first=await ticket(a),registered=await register(a,first),p='{'+f.redis.prefix+'}';const challengeWithHistory=await b.challenge(user,A,A,first.k);assert.notEqual(JSON.parse(Buffer.from(challengeWithHistory.challenge.split('.')[0]!,'base64url').toString())[6],'0');
 clock.now+=86399000;await b.publish(registered.session as DeviceSession,snapshot,registered.session.incarnation,1,0);await b.cleanup(0);assert.equal(await f.redis.client.hLen(p+':fences'),1);assert.equal(await f.redis.client.zScore(p+':devices:expiry',registered.session.deviceId!),registered.session.expiresAt);
 clock.now+=1000;await b.cleanup(0);assert.equal(await f.redis.client.hLen(p+':fences'),0);assert.equal(await f.redis.client.hLen(p+':admissions'),0);assert.equal(await f.redis.client.hLen(p+':sessions'),0);assert.equal(await f.redis.client.hLen(p+':devices'),0);
 await assert.rejects(register(b,first,9999),code('REGISTRATION_ADMISSION_EXPIRED'));for(const old of [first.challenge,challengeWithHistory])await assert.rejects(b.admit(user,A,A,first.k,old.challenge,randomUUID()),code('REGISTRATION_ADMISSION_EXPIRED'));
 const next=await ticket(b,first.k);assert.notEqual(JSON.parse(Buffer.from(next.admissionTicket.split('.')[0]!,'base64url').toString())[6],JSON.parse(Buffer.from(first.admissionTicket.split('.')[0]!,'base64url').toString())[6]);assert.equal((await register(a,next)).session.deviceId,registered.session.deviceId);
 // A recent nonzero challenge may outlive its old fence. Its non-reused CAS
 // value, rather than challenge expiry alone, must prevent recreating that fence.
 clock.now+=86399000;const recent=await a.challenge(user,A,A,first.k);clock.now+=1000;await a.cleanup(0);assert.equal(await f.redis.client.hLen(p+':fences'),0);await assert.rejects(b.admit(user,A,A,first.k,recent.challenge,randomUUID()),code('REGISTRATION_ADMISSION_CONFLICT'));
});
test('real Redis bounded windows permit more than 128 fresh keys while rejecting instant quota and retaining legacy fences',async t=>{
 const f=await fixture(t,{registrationKeysPerUser:128});if(!f)return;const {a,b,clock}=f,p='{'+f.redis.prefix+'}';
 for(let window=0;window<2;window++){for(let n=0;n<80;n++){const admitted=await ticket(n%2?a:b),r=await register(n%2?b:a,admitted);await b.revoke(r.session.hash);}assert.equal(await f.redis.client.hLen(p+':fences'),80);clock.now+=86400001;await a.cleanup(0);assert.equal(await f.redis.client.hLen(p+':fences'),0);}
 const small=f.store('finite-v2','small-'+randomUUID(),{registrationKeys:1,registrationKeysPerUser:1});await small.ready();await ticket(small);await assert.rejects(ticket(small),code('CAPACITY_EXCEEDED'));
 const legacy=f.store('legacy-v1','legacy-'+randomUUID());await legacy.ready();await legacy.register(user,A,'legacy',key(),randomUUID(),0,1);clock.now+=2*86400000;await legacy.cleanup(0);const entries=await f.redis.client.keys('{legacy-*}:fences');assert.equal(await f.redis.client.hLen(entries[0]!),1);
 await assert.rejects(f.store('legacy-v1').ready(),code('REGISTRATION_MODE_MISMATCH'));const alternate=f.store('finite-v2',entries[0]!.slice(1,entries[0]!.indexOf('}')));await assert.rejects(alternate.ready(),code('REGISTRATION_MODE_MISMATCH'));
});
test('real Redis mint capacity/wrongtype/counter exhaustion reject without partially advancing domain',async t=>{
 const f=await fixture(t,{idempotency:1});if(!f)return;const {a,redis}=f,k=key(),c=await a.challenge(user,A,A,k),id=randomUUID();await a.admit(user,A,A,k,c.challenge,id);const c2=await a.challenge(user,A,A,k),prefix='{'+redis.prefix+'}',before=await redis.client.hGetAll(prefix+':fences'),order=await redis.client.get(prefix+':admission-sequence');await assert.rejects(a.admit(user,A,A,k,c2.challenge,randomUUID()),code('CAPACITY_EXCEEDED'));assert.deepEqual(await redis.client.hGetAll(prefix+':fences'),before);assert.equal(await redis.client.get(prefix+':admission-sequence'),order);
 const other=f.store('finite-v2','overflow-'+randomUUID(),{});await other.ready();const otherPrefix=(await redis.client.keys('{overflow-*}:meta'))[0]!.replace(/:meta$/,'');await redis.client.set(otherPrefix+':admission-sequence',String(Number.MAX_SAFE_INTEGER));await assert.rejects(ticket(other),code('CAPACITY_EXCEEDED'));assert.equal(await redis.client.hLen(otherPrefix+':fences'),0);
 await redis.client.set(otherPrefix+':admissions','wrongtype');await assert.rejects(other.challenge(user,A,A,key()),code('COORDINATOR_UNAVAILABLE'));assert.equal(await redis.client.get(otherPrefix+':admission-sequence'),String(Number.MAX_SAFE_INTEGER));
});
test('Memory finite mode mirrors ticket lifetime, stable logical ID and conservative fence reclamation',async()=>{
 let now=Date.now();const s=new MemoryStore({registrationKeys:1},'finite-v2',()=>now),first=await ticket(s),a=await register(s,first);const c=await s.challenge(user,A,A,first.k);now+=120001;assert.equal((await s.admit(user,A,A,first.k,first.challenge.challenge,first.requestId)).admissionTicket,first.admissionTicket);await assert.rejects(ticket(s),code('CAPACITY_EXCEEDED'));await s.revoke(a.session.hash);now+=86400000;await s.cleanup(0);await assert.rejects(s.admit(user,A,A,first.k,c.challenge,randomUUID()),code('REGISTRATION_ADMISSION_EXPIRED'));const next=await ticket(s,first.k),b=await register(s,next);assert.equal(a.session.deviceId,b.session.deviceId);assert.notEqual(a.sessionKey,b.sessionKey);await s.revoke(a.session.hash);s.current(b.session,0);s.close();
});
test('real ES384 + HTTP two-handler challenge/mint/exchange/publish/discover enforce all identity and origin bounds',async t=>{
 const f=await fixture(t);if(!f)return;const pair=await generateKeyPair('ES384'),jwk={...await exportJWK(pair.publicKey),kid:'admission-fixture',alg:'ES384'},config=coordinatorAuthConfiguration({LOGTO_ISSUER:'https://fixture.invalid/oidc',LOGTO_BROWSER_CLIENT_ID:SPA}),identity=new LogtoIdentity(config,['ES384'],createLocalJWKSet({keys:[jwk]})),products=new ProductPolicy(JSON.stringify({version:1,products:[{id:A,name:'Workspace',nativeClientIds:[A,B],browserClientIds:[SPA]},{id:C,name:'Other',nativeClientIds:[C],browserClientIds:[SPA]}]}));
 const token=async(client=A,owner=user)=>new SignJWT({client_id:client,scope:'connect'}).setSubject(owner).setIssuer(config.issuer).setAudience(config.resource).setIssuedAt().setExpirationTime('1h').setProtectedHeader({alg:'ES384',kid:jwk.kid,typ:'JWT'}).sign(pair.privateKey),left=await http(t,createService({store:f.a,identity,directory:products,products,browserClientId:SPA,authRateLimit:1000})),right=await http(t,createService({store:f.b,identity,directory:products,products,browserClientId:SPA,authRateLimit:1000}));
 const req=async(base:string,path:string,method='GET',body?:unknown,bearer?:string,origin?:string)=>{const r=await fetch(base+'/api/v1'+path,{method,headers:{'content-type':'application/json','x-cybergrid-epoch':f.a.epoch,...(bearer?{authorization:'Bearer '+bearer}:{}),...(origin?{origin}:{})},body:body?JSON.stringify(body):undefined});return {status:r.status,data:await r.json() as any};};
 assert.deepEqual((await req(left,'/auth/config?appId='+B)).data.registrationAdmission,{version:2,challengeSeconds:120,ticketSeconds:86400});assert.ok(!('registrationAdmission'in (await req(left,'/console/auth/config')).data));
 const registerClient=async(client=A)=>{const accessToken=await token(client),registrationKey=key(),requestId=randomUUID(),common={appId:client,registrationKey,accessToken},c=await req(left,'/auth/registration/challenge','POST',common);assert.equal(c.status,200,JSON.stringify(c.data));const mintBody={...common,challenge:c.data.challenge,requestId:randomUUID()},m=await req(right,'/auth/registration/admit','POST',mintBody);assert.equal(m.status,200,JSON.stringify(m.data));const body={...common,coordinationProtocol:'memory-v1',coordinatorEpoch:f.a.epoch,deviceName:'fixture',registrationRevision:1,requestId,admissionTicket:m.data.admissionTicket},r=await req(left,'/auth/exchange','POST',body);assert.equal(r.status,200,JSON.stringify(r.data));return {...r.data,body,mintBody};};
 const a=await registerClient(A),b=await registerClient(B);for(const client of [a,b])assert.equal((await req(right,'/devices/self','PUT',{...snapshot,incarnation:client.incarnation,publishRevision:1},client.sessionKey)).status,200);assert.equal((await req(left,'/devices','GET',undefined,a.sessionKey)).data.devices.length,2);assert.equal((await req(right,'/auth/exchange','POST',a.body)).data.sessionKey,a.sessionKey);assert.equal((await req(left,'/auth/registration/admit','POST',a.mintBody)).data.admissionTicket,a.body.admissionTicket);
 for(const changes of [{accessToken:await token(A,'other-owner')},{accessToken:await token(B),appId:B},{accessToken:await token(C),appId:C},{registrationKey:key()},{admissionTicket:a.body.admissionTicket+'='}])assert.equal((await req(right,'/auth/exchange','POST',{...a.body,...changes})).data.error.code,'REGISTRATION_ADMISSION_INVALID');
 const noTicket={...a.body};delete noTicket.admissionTicket;assert.equal((await req(left,'/auth/exchange','POST',noTicket)).data.error.code,'REGISTRATION_ADMISSION_REQUIRED');assert.equal((await req(left,'/auth/registration/challenge','POST',{accessToken:await token(SPA),appId:SPA,registrationKey:key()})).status,404);assert.equal((await req(left,'/auth/registration/challenge','POST',{...a.body,accessToken:'unverified-token'})).status,401);assert.equal((await req(left,'/auth/registration/challenge','POST',a.body,undefined,'https://untrusted.invalid')).status,403);
 const forged=a.body.admissionTicket.slice(0,-1)+(a.body.admissionTicket.endsWith('A')?'B':'A');assert.equal((await req(right,'/auth/exchange','POST',{...a.body,admissionTicket:forged})).data.error.code,'REGISTRATION_ADMISSION_INVALID');
 const consoleReply=await req(left,'/console/auth/exchange','POST',{accessToken:await token(SPA),coordinationProtocol:'memory-v1',coordinatorEpoch:f.a.epoch,requestId:randomUUID()});assert.equal(consoleReply.status,200);assert.equal((await req(right,'/console/apps','GET',undefined,consoleReply.data.sessionKey)).status,200);
});

test('real Redis epoch reset and late register past ticket deadline fail at atomic commit',async t=>{
 const f=await fixture(t);if(!f)return;const admitted=await ticket(f.a),prefix='{'+f.redis.prefix+'}';
 // A valid Node verification precedes an actual Redis commit after the deadline.
 const transport=redisTransport({REDIS_URL:f.redis.url}),late=new RedisStore({eval(script,k,args){if(args[0]==='register')f.clock.now+=86400000;return transport.eval(script.replace(TIME,'local now='+f.clock.now),k,args);},close:()=>transport.close()},f.redis.prefix,{},'finite-v2');t.after(()=>late.close());await late.ready();await assert.rejects(register(late,admitted),code('REGISTRATION_ADMISSION_EXPIRED'));assert.equal(await f.redis.client.hLen(prefix+':sessions'),0);
 const oldEpoch=f.a.epoch;for(const k of await f.redis.client.keys(prefix+':*'))await f.redis.client.del(k);await f.b.ready();await f.a.ready();assert.notEqual(f.a.epoch,oldEpoch);await assert.rejects(register(f.a,admitted),code('REGISTRATION_ADMISSION_INVALID'));await assert.rejects(f.a.admit(user,A,A,admitted.k,admitted.challenge.challenge,admitted.requestId),code('REGISTRATION_ADMISSION_INVALID'));
});
test('real Redis GC holds a finite fence while any related session remains valid',async t=>{
 const f=await fixture(t);if(!f)return;const admitted=await ticket(f.a),active=await register(f.a,admitted),prefix='{'+f.redis.prefix+'}',logical=active.session.deviceId!;
 // Exercise the conservative session guard independently of the normal stronger
 // invariant maxIssuedNotAfter >= every related session expiry.
 const fence=JSON.parse((await f.redis.client.hGet(prefix+':fences',logical))!);await f.redis.client.hSet(prefix+':fences',logical,JSON.stringify({...fence,maxIssuedNotAfter:f.clock.now}));await f.b.cleanup(0);assert.equal(await f.redis.client.hExists(prefix+':fences',logical),1);await f.b.current(active.session,0);await f.redis.client.hSet(prefix+':fences',logical,JSON.stringify(fence));
 f.clock.now=active.session.expiresAt;await f.b.cleanup(0);assert.equal(await f.redis.client.hExists(prefix+':fences',logical),0);
});

test('Memory finite revoke reclaims expiry indexes across windows and old close preserves the new incarnation',async()=>{
 let now=Date.now();const s=new MemoryStore({registrationKeys:1,registrationKeysPerUser:1},'finite-v2',()=>now);
 const state=s as unknown as {deviceExpiries:Map<string,number>;fences:Map<string,unknown>;admissions:Map<string,unknown>;registrations:Map<string,unknown>};
 try{
  for(let window=0;window<3;window++){
   const first=await ticket(s),old=await register(s,first);assert.equal(state.deviceExpiries.size,1);
   now+=1000;const renewed=await ticket(s,first.k),current=await register(s,renewed);assert.equal(current.session.deviceId,old.session.deviceId);assert.notEqual(current.session.incarnation,old.session.incarnation);
   const deadline=state.deviceExpiries.get(current.session.deviceId!);assert.equal(deadline,current.session.expiresAt);
   await s.revoke(old.session.hash);assert.equal(state.deviceExpiries.size,1);assert.equal(state.deviceExpiries.get(current.session.deviceId!),deadline);s.current(current.session,now);
   await s.revoke(current.session.hash);assert.equal(state.deviceExpiries.size,0);assert.equal((await s.devices(user,A,now)).length,0);
   now+=86400001;await s.cleanup(now);assert.equal(state.deviceExpiries.size,0);assert.equal(state.fences.size,0);assert.equal(state.admissions.size,0);assert.equal(state.registrations.size,0);
  }
 }finally{s.close();}
});
