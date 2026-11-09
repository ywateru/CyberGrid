import {test} from 'node:test';
import {redisFixture as fixture} from './redis-fixture.js';
import assert from 'node:assert/strict';
import {spawn} from 'node:child_process';
import {createServer} from 'node:net';
import {mkdtemp,rm} from 'node:fs/promises';
import {tmpdir} from 'node:os';
import {join} from 'node:path';
import {randomBytes,randomUUID,createHash} from 'node:crypto';
import {createClient} from 'redis';
import {RedisStore,redisTransport} from '../src/redis.js';
import {createService} from '../src/service.js';
import {ApiError,type DeviceSession} from '../src/types.js';
import {FixtureIdentity,FixtureDirectory,A,SPA} from './fixtures.js';
const snap={ports:[{id:'echo',port:6789,type:'echo',transport:'tcp' as const,tunnelUrl:'https://fixture-echo.trycloudflare.com'}],certificateFingerprint:'a'.repeat(64),tunnelUrl:null,iroh:null};
const code=(c:string)=>(e:unknown)=>e instanceof ApiError&&e.code===c;
const delay=(ms:number)=>new Promise(r=>setTimeout(r,ms));
const hash=(s:string)=>createHash('sha256').update(s).digest('hex');
async function register(s:RedisStore,key=randomBytes(32).toString('base64url'),revision=1,requestId=randomUUID(),user='user'){await s.ready();const r=await s.register(user,A,'fixture',key,requestId,0,revision);return {...r,session:r.session as DeviceSession,key,revision,requestId};}
test('real Redis two handlers share discovery/signals/session across cold store and revoke fences',async t=>{
 const f=await fixture(t);if(!f)return;const left=f.store({},f.prefix),right=f.store({},f.prefix),identity=new FixtureIdentity(),directory=new FixtureDirectory();await identity.register('fixture@example.test');
 const h1=createService({store:left,identity,directory,browserClientId:SPA,authRateLimit:100}),h2=createService({store:right,identity,directory,browserClientId:SPA,authRateLimit:100});await left.ready();
 const req=(h:typeof h1,path:string,method='GET',body?:unknown,key?:string)=>h(new Request('https://fixture.example/api/v1'+path,{method,headers:{'content-type':'application/json','x-cybergrid-epoch':left.epoch,...(key?{authorization:'Bearer '+key}:{})},body:body?JSON.stringify(body):undefined}),'fixture');
 const login=async(h:typeof h1)=>{const r=await req(h,'/auth/exchange','POST',{accessToken:identity.tokenFor('fixture@example.test'),appId:A,deviceName:'fixture',coordinationProtocol:'memory-v1',coordinatorEpoch:left.epoch,registrationKey:randomBytes(32).toString('base64url'),registrationRevision:1,requestId:randomUUID()});assert.equal(r.status,200,await r.clone().text());return r.json() as Promise<any>;};
 const a=await login(h1),b=await login(h2);assert.equal(left.epoch,right.epoch);for(const [h,d] of [[h2,a],[h1,b]] as const)assert.equal((await req(h,'/devices/self','PUT',{...snap,incarnation:d.incarnation,publishRevision:1},d.sessionKey)).status,200);
 assert.equal((await (await req(h2,'/devices','GET',undefined,a.sessionKey)).json() as any).devices.length,2);
 const msg={toDeviceId:b.deviceId,toIncarnation:b.incarnation,connectionId:'fixture',kind:'offer',payload:'fixture signal',clientMessageId:randomUUID()};const post=await req(h1,'/signals','POST',msg,a.sessionKey);assert.equal(post.status,201);const first=await post.json() as any;assert.equal((await (await req(h2,'/signals','POST',msg,a.sessionKey)).json() as any).id,first.id);assert.equal((await (await req(h2,'/signals','GET',undefined,b.sessionKey)).json() as any).signals[0].payload,msg.payload);
 const other=await register(right,undefined,1,undefined,'other');assert.equal((await right.devices(other.session.userId,A,0,other.session)).length,1);
 const restart=f.store({},f.prefix);await restart.ready();assert.equal(restart.epoch,left.epoch);await restart.current((await restart.session(hash(a.sessionKey)))!,0);await right.revoke(hash(a.sessionKey));assert.equal((await req(h1,'/devices/self','PUT',{...snap,incarnation:a.incarnation,publishRevision:2},a.sessionKey)).status,401);assert.equal(await left.device(a.deviceId,0),undefined);
});
test('real Redis atomic registration/publication races, replay, capacity, permanent fences',async t=>{
 const f=await fixture(t);if(!f)return;const a=f.store({sessions:1,sessionsPerUser:1,registrationKeys:1},f.prefix),b=f.store({sessions:1,sessionsPerUser:1,registrationKeys:1},f.prefix),key=randomBytes(32).toString('base64url'),id=randomUUID();const rs=await Promise.all([register(a,key,1,id),register(b,key,1,id)]);assert.equal(rs[0]!.sessionKey,rs[1]!.sessionKey);assert.equal(rs[0]!.session.incarnation,rs[1]!.session.incarnation);
 const first=rs[0]!,second=await register(b,key,2);await assert.rejects(a.publish(first.session,snap,first.session.incarnation,1,0),code('INVALID_SESSION'));await assert.rejects(register(a,key,1,id),code('SESSION_SUPERSEDED'));await assert.rejects(register(a),code('CAPACITY_EXCEEDED'));
 const ps=await Promise.allSettled([a.publish(second.session,snap,second.session.incarnation,1,0),b.publish(second.session,{...snap,ports:[]},second.session.incarnation,1,0)]);assert.equal(ps.filter(r=>r.status==='fulfilled').length,1);assert.equal(ps.filter(r=>r.status==='rejected'&&code('PUBLISH_REVISION_CONFLICT')(r.reason)).length,1);
 const before=await a.device(second.session.deviceId,0);await assert.rejects(a.publish(second.session,{...snap,ports:Array.from({length:32},(_,i)=>({id:String(i),port:i+1,type:'\u0001'.repeat(128),transport:'tcp' as const,tunnelUrl:'https://'+'a'.repeat(480)+'.trycloudflare.com'}))},second.session.incarnation,2,0),code('DEVICE_METADATA_TOO_LARGE'));assert.deepEqual(await b.device(second.session.deviceId,0),before);
 await a.revoke(second.session.hash);await assert.rejects(register(b,key,2,second.requestId),code('REGISTRATION_ATTEMPT_EXPIRED'));assert.equal((await register(a,key,3)).session.deviceId,first.session.deviceId);
});
test('real Redis TTL/index cleanup, distributed waiter cancellation, failclosed and no partial snapshot',async t=>{
 const f=await fixture(t);if(!f)return;const a=f.store({waiters:1},f.prefix),b=f.store({waiters:1},f.prefix),first=await register(a),second=await register(b);for(const d of [first,second])await a.publish(d.session,snap,d.session.incarnation,1,0);
 const abort=new AbortController(),waiting=a.wait(first.session,0,15,Date.now,abort.signal);for(let i=0;i<40;i++){if((await b.adminOverview(0,{page:1,pageSize:10})).totals.waiters===1)break;await delay(20);}await assert.rejects(b.wait(second.session,0,15,Date.now,new AbortController().signal),code('CAPACITY_EXCEEDED'));abort.abort();await waiting;assert.equal((await b.adminOverview(0,{page:1,pageSize:10})).totals.waiters,0);
 const prefix='{'+f.prefix+'}';await f.client.zAdd(prefix+':sessions:expiry',{score:0,value:first.session.hash});await b.cleanup(0);assert.equal(await a.session(first.session.hash),undefined);assert.equal((await b.device(first.session.deviceId,0))?.online,false);await f.client.zAdd(prefix+':devices:expiry',{score:0,value:first.session.deviceId});await a.cleanup(0);assert.equal(await b.device(first.session.deviceId,0),undefined);
 const broken=new RedisStore({eval:async()=>{throw Error('private provider detail');},close(){}},'failure');await assert.rejects(broken.ready(),e=>code('COORDINATOR_UNAVAILABLE')(e)&&!(e as Error).message.includes('private'));broken.close();await f.client.set(prefix+':dedup','wrongtype');const before=await f.client.hGet(prefix+':devices',second.session.deviceId);await assert.rejects(a.publish(second.session,{...snap,ports:[]},second.session.incarnation,2,0),code('COORDINATOR_UNAVAILABLE'));assert.equal(await f.client.hGet(prefix+':devices',second.session.deviceId),before);
});
test('Redis transport config rejects ambiguity and namespace injection',()=>{
 for(const env of [{},{REDIS_URL:'redis://localhost',UPSTASH_REDIS_REST_URL:'https://fixture.invalid',UPSTASH_REDIS_REST_TOKEN:'fixture'},{UPSTASH_REDIS_REST_URL:'https://fixture.invalid'},{UPSTASH_REDIS_REST_URL:'http://fixture.invalid',UPSTASH_REDIS_REST_TOKEN:'fixture'}])assert.throws(()=>redisTransport(env));assert.throws(()=>new RedisStore({eval:async()=>'',close(){}},'bad{slot}'));assert.throws(()=>new RedisStore({eval:async()=>'',close(){}},'valid',{sessions:100000000}));
});
test('real Redis reset fencing pins original request epoch despite concurrent ready and partial loss fails closed',async t=>{
 const f=await fixture(t);if(!f)return;const a=f.store({},f.prefix),b=f.store({},f.prefix),first=await register(a),oldEpoch=a.epoch;
 for(const key of await f.client.keys('{'+f.prefix+'}:*'))await f.client.del(key);
 await b.ready();await a.ready();assert.notEqual(a.epoch,oldEpoch);
 await assert.rejects(a.register('user',A,'fixture',first.key,randomUUID(),0,2,oldEpoch),code('COORDINATOR_RESET'));assert.equal((await a.adminOverview(0,{page:1,pageSize:10})).totals.sessions,0);
 const second=await register(a);await f.client.del(['{'+f.prefix+'}:meta','{'+f.prefix+'}:initialized']);await assert.rejects(b.ready(),code('COORDINATOR_UNAVAILABLE'));assert.ok(await f.client.hGet('{'+f.prefix+'}:sessions',second.session.hash));
});
test('real Redis signal/rate/receipt expiry is reclaimed atomically without reviving an old registration fence',async t=>{
 const f=await fixture(t);if(!f)return;const s=f.store({signals:1,signalsPerDevice:1},f.prefix),a=await register(s),b=await register(s);for(const r of [a,b])await s.publish(r.session,snap,r.session.incarnation,1,0);
 const message={toDeviceId:b.session.deviceId,toIncarnation:b.session.incarnation,connectionId:'fixture',kind:'offer' as const,payload:'fixture'},id=randomUUID(),signal=await s.postSignal(a.session,message,id,0);await assert.rejects(s.postSignal(a.session,message,randomUUID(),0),code('CAPACITY_EXCEEDED'));
 const prefix='{'+f.prefix+'}';await f.client.zAdd(prefix+':signals:expiry',{score:0,value:String(signal)});assert.equal((await s.signals(b.session.deviceId,0,0,b.session)).length,0);assert.ok(await s.postSignal(a.session,message,randomUUID(),0)>signal);
 assert.equal(await s.rate('fixture',0,1),true);assert.equal(await s.rate('fixture',0,1),false);for(const key of await f.client.zRange(prefix+':rates:expiry',0,-1))await f.client.zAdd(prefix+':rates:expiry',{score:0,value:key});assert.equal(await s.rate('fixture',0,1),true);
 for(const key of await f.client.zRange(prefix+':registrations:expiry',0,-1))await f.client.zAdd(prefix+':registrations:expiry',{score:0,value:key});await s.cleanup(0);await assert.rejects(register(s,a.key,1,a.requestId),code('REGISTRATION_ATTEMPT_EXPIRED'));assert.equal((await register(s,a.key,2)).session.deviceId,a.session.deviceId);
});
