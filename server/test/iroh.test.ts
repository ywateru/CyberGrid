import {EndpointPolicy} from '../src/endpoints.js';
import {test} from 'node:test';
import assert from 'node:assert/strict';
import {randomBytes,randomUUID} from 'node:crypto';
import {irohReachability,IROH_MAX_METADATA_BYTES} from '../src/iroh.js';
import {MemoryStore} from '../src/memory.js';
import {createService} from '../src/service.js';
import {ApiError,ONLINE_TTL,SESSION_TTL,type DeviceSession} from '../src/types.js';
import {A,B,SPA,FixtureIdentity,FixtureDirectory} from './fixtures.js';

const iroh={endpointId:'a'.repeat(64),addresses:['192.0.2.1:7410','[2001:db8::1]:7410']};
const snapshot={ports:[],tunnelUrl:null,certificateFingerprint:'b'.repeat(64)};
const key=()=>randomBytes(32).toString('base64url');
const ADMIN_KEY='x'.repeat(32);
const errorCode=(code:string)=>(e:unknown)=>e instanceof ApiError&&e.code===code;
async function fixture(endpointPolicy?:EndpointPolicy){
 const store=new MemoryStore(),identity=new FixtureIdentity(),directory=new FixtureDirectory();
 await identity.register('one@example.test');await identity.register('two@example.test');
 let time=Date.now();
 const handle=createService({store,identity,directory,endpointPolicy,browserClientId:SPA,now:()=>time,adminKey:ADMIN_KEY,authRateLimit:1000});
 const request=async(path:string,method='GET',body?:unknown,bearer?:string,epoch=store.epoch)=>{
  const response=await handle(new Request('https://grid.example/api/v1'+path,{method,headers:{'content-type':'application/json','x-cybergrid-epoch':epoch,...(bearer?{authorization:'Bearer '+bearer}:{})},body:body===undefined?undefined:JSON.stringify(body)}),'iroh-fixture');
  return {status:response.status,data:await response.json() as any};
 };
 const login=(email='one@example.test',appId=A,extra:Record<string,unknown>={})=>request('/auth/exchange','POST',{accessToken:identity.tokenFor(email,appId),appId,deviceName:'peer',coordinationProtocol:'memory-v1',coordinatorEpoch:store.epoch,registrationKey:key(),requestId:randomUUID(),registrationRevision:1,...extra});
 const publish=(session:any,extra:Record<string,unknown>={})=>request('/devices/self','PUT',{...snapshot,incarnation:session.incarnation,publishRevision:1,...extra},session.sessionKey);
 return {store,identity,directory,request,login,publish,advance:(ms:number)=>{time+=ms;},now:()=>time};
}

test('iroh hints are optional, bounded numeric socket addresses with no name or relay resolution',()=>{
 assert.equal(irohReachability(undefined),null);assert.equal(irohReachability(null),null);
 const input={endpointId:iroh.endpointId,addresses:['127.0.0.1:1','[::1]:65535','192.168.0.4:8080','[::ffff:192.0.2.1]:80']};
 assert.deepEqual(irohReachability(input),input);
 assert.deepEqual(irohReachability({endpointId:iroh.endpointId,addresses:[]}),{endpointId:iroh.endpointId,addresses:[]});
 const maximum={endpointId:iroh.endpointId,addresses:Array.from({length:8},(_,i)=>`[2001:ffff:ffff:ffff:ffff:ffff:255.255.255.24${i}]:65535`)};
 assert.deepEqual(irohReachability(maximum),maximum);assert.ok(Buffer.byteLength(JSON.stringify(maximum))<IROH_MAX_METADATA_BYTES);
 const copy=irohReachability(input)!;copy.addresses.push('192.0.2.9:9');assert.equal(input.addresses.length,4);
 for(const bad of [false,'opaque',[],{}, {...iroh,endpointId:'A'.repeat(64)}, {...iroh,endpointId:'a'.repeat(64)+'\n'}, {...iroh,relayUrl:'https://relay.example'}, {...iroh,arbitrary:{data:'payload'}}, {endpointId:iroh.endpointId,addresses:Array(9).fill('192.0.2.1:1')}])assert.throws(()=>irohReachability(bad),errorCode('INVALID_INPUT'));
 for(const address of ['localhost:80','example.test:443','https://192.0.2.1:80','192.0.2.1:80/path','user@192.0.2.1:80','192.0.2.1:0','192.0.2.1:65536','192.0.2.1:080','192.0.2.1:80\n',' 192.0.2.1:80','192.000.2.1:80','[fe80::1%eth0]:80','2001:db8::1:80','[::]:80','0.0.0.0:80','224.0.0.1:80','[ff02::1]:80','[::ffff:224.0.0.1]:80','[::ffff:e000:1]:80','[::ffff:0.0.0.0]:80','[::ffff:0:0]:80','x'.repeat(2000)])assert.throws(()=>irohReachability({...iroh,addresses:[address]}),errorCode('INVALID_INPUT'),address);
 for(const addresses of [['192.0.2.1:1','192.0.2.1:1'],['[2001:db8::1]:80','[2001:0db8:0:0:0:0:0:1]:80'],['192.0.2.1:80','[::ffff:c000:201]:80']])assert.throws(()=>irohReachability({...iroh,addresses}),errorCode('INVALID_INPUT'));
});

test('iroh metadata obeys full-snapshot revisions; omitted old-client fields clear hints without changing the TLS pin',async t=>{
 const f=await fixture();t.after(()=>f.store.close());const a=(await f.login()).data;
 assert.equal((await f.request('/devices','GET',undefined,a.sessionKey)).data.devices[0].iroh,null);
 const first=await f.publish(a,{iroh});assert.equal(first.status,200);assert.deepEqual(first.data.device.iroh,iroh);
 const revision=f.store.revision;
 assert.equal((await f.publish(a,{iroh})).status,200);assert.equal(f.store.revision,revision);
 for(const changed of [{...iroh,endpointId:'c'.repeat(64)},{...iroh,addresses:[]}]){
  const conflict=await f.publish(a,{iroh:changed});assert.equal(conflict.status,409);assert.equal(conflict.data.error.code,'PUBLISH_REVISION_CONFLICT');
 }
 const invalid=await f.publish(a,{iroh:{...iroh,relayUrl:'https://relay.example'},publishRevision:2});assert.equal(invalid.status,400);assert.equal(f.store.revision,revision);
 assert.deepEqual((await f.store.device(a.deviceId,f.now()))?.iroh,iroh);
 f.advance(ONLINE_TTL+1);
 const offlineConflict=await f.publish(a,{iroh:{...iroh,endpointId:'c'.repeat(64)}});assert.equal(offlineConflict.status,409);
 assert.equal((await f.store.device(a.deviceId,f.now()))?.online,false);assert.equal(f.store.revision,revision);
 const cleared=await f.publish(a,{publishRevision:2});assert.equal(cleared.status,200);assert.equal(cleared.data.device.iroh,null);assert.equal(cleared.data.device.certificateFingerprint,snapshot.certificateFingerprint);assert.ok(f.store.revision>revision);
 const noChange=f.store.revision;assert.equal((await f.publish(a,{iroh:null,publishRevision:2})).status,200);assert.equal(f.store.revision,noChange);
 const stale=await f.publish(a,{iroh,publishRevision:1});assert.equal(stale.status,409);assert.equal(stale.data.error.code,'STALE_PUBLISH_REVISION');
 const epoch=await f.request('/devices/self','PUT',{...snapshot,iroh,incarnation:a.incarnation,publishRevision:3},a.sessionKey,randomUUID());assert.equal(epoch.status,409);assert.equal(epoch.data.error.code,'COORDINATOR_RESET');
 assert.equal((await f.store.device(a.deviceId,f.now()))?.iroh,null);
 // Legacy clients also omit the field outside memory-v1 and receive JSON null.
 const legacy=(await f.request('/auth/exchange','POST',{accessToken:f.identity.tokenFor('one@example.test'),appId:A,deviceName:'legacy'})).data;
 assert.equal((await f.publish(legacy)).data.device.iroh,null);
});

test('iroh hints are restricted to same-account/app device sessions, absent from console and admin',async t=>{
 const f=await fixture();t.after(()=>f.store.close());const a=(await f.login()).data,same=(await f.login()).data,otherUser=(await f.login('two@example.test')).data,otherApp=(await f.login('one@example.test',B)).data;
 await f.publish(a,{iroh});
 const own=(await f.request('/devices','GET',undefined,same.sessionKey)).data.devices;
 assert.deepEqual(own.find((d:any)=>d.id===a.deviceId).iroh,iroh);
 for(const session of [otherUser,otherApp])assert.equal((await f.request('/devices','GET',undefined,session.sessionKey)).data.devices.some((d:any)=>d.id===a.deviceId),false);
 const c=(await f.request('/console/auth/exchange','POST',{accessToken:f.identity.tokenFor('one@example.test',SPA)})).data;
 const consoleDevices=(await f.request('/console/devices?appId='+A,'GET',undefined,c.sessionKey)).data.devices;
 assert.equal(consoleDevices.find((d:any)=>d.id===a.deviceId).iroh,undefined);
 for(const d of consoleDevices)assert.equal(Object.hasOwn(d,'iroh'),false);
 assert.equal((await f.request('/console/devices?appId='+B,'GET',undefined,c.sessionKey)).data.devices.some((d:any)=>d.id===a.deviceId),false);
 const admin=await f.request('/admin/overview','GET',undefined,ADMIN_KEY);assert.equal(admin.status,200);
 const serialized=JSON.stringify(admin.data);for(const field of ['iroh','endpointId','addresses',iroh.endpointId,...iroh.addresses])assert.equal(serialized.includes(field),false,field);
 f.advance(ONLINE_TTL+1);
 const offline=(await f.request('/devices','GET',undefined,same.sessionKey)).data.devices.find((d:any)=>d.id===a.deviceId);assert.equal(offline.online,false);assert.deepEqual(offline.iroh,iroh);
 f.advance(SESSION_TTL);await f.store.cleanup(f.now());assert.equal(await f.store.device(a.deviceId,f.now()),undefined);
});

test('takeover resets hints and late directory awaits cannot restore old generation metadata',async t=>{
 const f=await fixture();t.after(()=>f.store.close());const registrationKey=key();const a=(await f.login('one@example.test',A,{registrationKey})).data;
 await f.publish(a,{iroh});const replacement=(await f.login('one@example.test',A,{registrationKey,registrationRevision:2})).data;
 assert.equal(replacement.deviceId,a.deviceId);assert.notEqual(replacement.incarnation,a.incarnation);
 const reset=await f.store.device(a.deviceId,f.now());assert.equal(reset?.iroh,null);assert.equal(reset?.certificateFingerprint,null);assert.equal(reset?.publishRevision,0);
 assert.equal((await f.publish(a,{iroh,publishRevision:2})).status,401);
 const wrong=await f.publish(replacement,{iroh,incarnation:a.incarnation});assert.equal(wrong.status,409);assert.equal(wrong.data.error.code,'INCARNATION_MISMATCH');
 let enter!:()=>void,release!:()=>void;const entry=new Promise<void>(r=>enter=r),gate=new Promise<void>(r=>release=r),original=f.directory.list.bind(f.directory);
 f.directory.list=async()=>{enter();await gate;return original();};
 const late=f.publish(replacement,{iroh});await entry;await f.request('/session','DELETE',undefined,replacement.sessionKey);release();assert.equal((await late).status,401);assert.equal(await f.store.device(replacement.deviceId,f.now()),undefined);
});

test('bounded peer capacity includes iroh and snapshots do not alias input or discovery output',async t=>{
 const store=new MemoryStore();t.after(()=>store.close());const now=Date.now(),metadata={endpointId:iroh.endpointId,addresses:Array.from({length:8},(_,i)=>`192.0.2.${i+1}:65535`)};
 for(let i=0;i<64;i++){
  const r=store.register('user',A,'peer',key(),randomUUID(),now,1),session=r.session as DeviceSession;
  await store.publish(session,{...snapshot,iroh:metadata},session.incarnation,1,now);
 }
 const peers=await store.devices('user',A,now);assert.equal(peers.length,64);assert.ok(Buffer.byteLength(JSON.stringify(peers))<128*1024);
 assert.throws(()=>store.register('user',A,'overflow',key(),randomUUID(),now,1),errorCode('CAPACITY_EXCEEDED'));
 const id=peers[0]!.id;metadata.addresses[0]='203.0.113.1:9';peers[0]!.iroh!.addresses[0]='203.0.113.2:9';assert.equal((await store.device(id,now))?.iroh?.addresses[0],'192.0.2.1:65535');
 await store.heartbeat(id,snapshot,now+1);assert.equal((await store.device(id,now+1))?.iroh,null);assert.equal((await store.device(id,now+1))?.certificateFingerprint,snapshot.certificateFingerprint);
 const low=new MemoryStore({devices:1});t.after(()=>low.close());low.register('one',A,'one',key(),randomUUID(),now,1);assert.throws(()=>low.register('two',A,'two',key(),randomUUID(),now,1),errorCode('CAPACITY_EXCEEDED'));
});

test('explicit per-port tunnel roots survive discovery and recovery independently of the encrypted device tunnel',async t=>{
 const f=await fixture();t.after(()=>f.store.close());const registrationKey=key(),a=(await f.login('one@example.test',A,{registrationKey})).data;
 const ports=[{id:'web',port:8080,type:'http',transport:'tcp',tunnelUrl:'https://port-web.trycloudflare.com/'}],deviceTunnel='https://encrypted-device.trycloudflare.com';
 const first=await f.publish(a,{ports,tunnelUrl:deviceTunnel});assert.equal(first.status,200);assert.equal(first.data.device.ports[0].tunnelUrl,'https://port-web.trycloudflare.com');assert.equal(first.data.device.tunnelUrl,deviceTunnel);
 const c=(await f.request('/console/auth/exchange','POST',{accessToken:f.identity.tokenFor('one@example.test',SPA)})).data;
 assert.equal((await f.request('/console/devices?appId='+A,'GET',undefined,c.sessionKey)).data.devices[0].ports[0].tunnelUrl,'https://port-web.trycloudflare.com');
 const other=(await f.login('two@example.test')).data;assert.equal((await f.request('/devices','GET',undefined,other.sessionKey)).data.devices.some((d:any)=>d.id===a.deviceId),false);
 const conflict=await f.publish(a,{ports:ports.map(p=>({...p,tunnelUrl:null})),tunnelUrl:deviceTunnel});assert.equal(conflict.status,409);assert.equal(conflict.data.error.code,'PUBLISH_REVISION_CONFLICT');
 const before=f.store.revision;const cleared=await f.publish(a,{ports:ports.map(p=>({...p,tunnelUrl:null})),tunnelUrl:deviceTunnel,publishRevision:2});assert.equal(cleared.status,200);assert.equal(cleared.data.device.ports[0].tunnelUrl,undefined);assert.equal(cleared.data.device.tunnelUrl,deviceTunnel);assert.ok(f.store.revision>before);
 const replacement=(await f.login('one@example.test',A,{registrationKey,registrationRevision:2})).data;assert.deepEqual((await f.store.device(a.deviceId,f.now()))?.ports,[]);
 const restored=await f.publish(replacement,{ports,tunnelUrl:deviceTunnel});assert.equal(restored.status,200);assert.equal(restored.data.device.ports[0].tunnelUrl,'https://port-web.trycloudflare.com');
 const admin=await f.request('/admin/overview','GET',undefined,ADMIN_KEY);assert.equal(admin.status,200);assert.equal(JSON.stringify(admin.data).includes('port-web'),false);
});

test('per-port tunnel declarations reject non-root or non-Cloudflare URLs without mutating the snapshot',async t=>{
 const f=await fixture();t.after(()=>f.store.close());const a=(await f.login()).data,port={id:'web',port:8080,type:'http',transport:'tcp'};
 await f.publish(a,{ports:[port]});const before=f.store.revision;
 for(const tunnelUrl of ['http://port.trycloudflare.com','https://other.example','https://port.trycloudflare.com.evil','https://user@port.trycloudflare.com','https://port.trycloudflare.com/path','https://port.trycloudflare.com?token=1','https://port.trycloudflare.com#fragment','https://port.trycloudflare.com:8443',12,{url:'https://port.trycloudflare.com'}]){const r=await f.publish(a,{ports:[{...port,tunnelUrl}],publishRevision:2});assert.equal(r.status,400);}
 assert.equal(f.store.revision,before);assert.deepEqual((await f.store.device(a.deviceId,f.now()))?.ports,[port]);
});

test('serialized publication budget includes escaped port strings and URLs, and rejected updates never refresh revision or lastSeen',async t=>{
 const origin='https://'+Array(3).fill('p'.repeat(60)).join('.')+'.example.test',path='/'+'a'.repeat(255),url=origin+path;
 const f=await fixture(new EndpointPolicy(JSON.stringify({version:1,providers:[{id:'budget',origins:[origin],paths:[path]}]})));t.after(()=>f.store.close());const a=(await f.login()).data;
 const make=(n:number)=>Array.from({length:32},(_,i)=>({id:'port-'+i,port:1000+i,type:'\u0001'.repeat(n),transport:'tcp',tunnelUrl:url}));
 const accepted=await f.publish(a,{ports:make(20),iroh});assert.equal(accepted.status,200);
 const before=await f.store.device(a.deviceId,f.now()),revision=f.store.revision;f.advance(1000);
 const rejected=await f.publish(a,{ports:make(128),iroh,publishRevision:2});assert.equal(rejected.status,413);assert.equal(rejected.data.error.code,'DEVICE_METADATA_TOO_LARGE');assert.equal(f.store.revision,revision);
 const after=await f.store.device(a.deviceId,f.now());assert.deepEqual(after,before);
 assert.equal((await f.publish(a,{ports:make(20),iroh,publishRevision:2})).status,200);
});

test('custom larger peer limits return an explicit directory capacity error for both device and console projections',async t=>{
 const store=new MemoryStore({devicesPerUser:128,sessionsPerUser:256,registrationKeysPerUser:256});t.after(()=>store.close());const now=Date.now(),identity=new FixtureIdentity(),directory=new FixtureDirectory();const user=(await identity.register('capacity@example.test')).user;
 const origin='https://'+Array(3).fill('a'.repeat(60)).join('.')+'.example.test',path='/'+'p'.repeat(255),endpointPolicy=new EndpointPolicy(JSON.stringify({version:1,providers:[{id:'budget',origins:[origin],paths:[path]}]}));
 const ports=Array.from({length:32},(_,i)=>({id:'p'.repeat(60)+i,port:1000+i,type:'t'.repeat(128),transport:'tcp' as const,tunnelUrl:origin+path}));let bearer='';
 for(let i=0;i<100;i++){const r=store.register(user.id,A,'peer',key(),randomUUID(),now,1);bearer=r.sessionKey;await store.publish(r.session as DeviceSession,{...snapshot,ports},(r.session as DeviceSession).incarnation,1,now);}
 const handle=createService({store,identity,directory,endpointPolicy,browserClientId:SPA,now:()=>now});
 const get=(path:string,token:string)=>handle(new Request('https://grid.example/api/v1'+path,{headers:{authorization:'Bearer '+token,'x-cybergrid-epoch':store.epoch}}));
 const c=store.register(user.id,null,'',undefined,randomUUID(),now);
 for(const [path,token] of [['/devices',bearer],['/console/devices?appId='+A,c.sessionKey]]){const response=await get(path!,token!);assert.equal(response.status,413);assert.equal((await response.json() as any).error.code,'DIRECTORY_TOO_LARGE');}
});
