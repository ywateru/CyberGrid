import {test} from 'node:test';
import assert from 'node:assert/strict';
import {inspect} from 'node:util';
import {CyberGridClient,CyberGridError} from '../dist/index.js';
const epoch='12345678-1234-4234-8234-123456789012',otherEpoch='12345678-1234-4234-8234-123456789013';
const key='a'.repeat(43),baseUrl='https://grid.example',issuer='https://tenant.example/oidc',clientId='ConsoleSPA',resource=baseUrl+'/api';
const config=(e=epoch)=>({provider:'logto',issuer,clientId,resource,scope:'openid offline_access profile email connect',authorizationEndpoint:issuer+'/auth',tokenEndpoint:issuer+'/token',revocationEndpoint:issuer+'/token/revocation',jwksUri:issuer+'/jwks',coordination:{protocol:'memory-v1',epoch:e}});
const receipt=(e=epoch,subject='user-a',sessionKey=key)=>({purpose:'console',sessionKey,user:{id:subject},expiresAt:new Date(Date.now()+3600000).toISOString(),coordinatorEpoch:e});
const json=(data,e=epoch,status=200)=>new Response(JSON.stringify(data),{status,headers:{'x-cybergrid-epoch':e,'content-type':'application/json'}});
const deferred=()=>{let resolve,reject;const promise=new Promise((a,b)=>{resolve=a;reject=b;});return {promise,resolve,reject};};
const turn=()=>new Promise(r=>setImmediate(r));
const code=(wanted)=>(e)=>e instanceof CyberGridError&&e.code===wanted&&!/fixture-secret/.test(e.message);
function fixture(t,handler,tokenProvider=async()=> 'fixture-secret-token',timeoutMs=1000){
 const original=globalThis.fetch,requests=[];globalThis.fetch=async(url,init)=>{requests.push({url,init});assert.equal(init.redirect,'error');assert.equal(init.credentials,'omit');return handler(new URL(url).pathname.replace('/api/v1','')+new URL(url).search,init,requests.length);};t.after(()=>{globalThis.fetch=original;});return {client:new CyberGridClient({baseUrl,issuer,clientId,resource,getAccessToken:tokenProvider,timeoutMs}),requests};
}
function normal(path){if(path.startsWith('/console/auth/config?'))return json(config());if(path==='/console/auth/exchange')return json(receipt());if(path==='/console/apps')return json({source:'logto',apps:[{id:'AppA',name:'Demo',type:'Native',isDeviceFlow:false}]});if(path==='/console/session')return json({ok:true});throw Error('Unexpected route '+path);}
test('trusted pins and fixed resource scopes precede host token; private state cannot serialize or inspect',async t=>{
 let tokens=0;const {client,requests}=fixture(t,normal,async args=>{tokens++;assert.deepEqual(args.scopes,['connect']);assert.equal(args.resource,resource);assert.equal(args.clientId,clientId);assert.equal(args.issuer,issuer);return 'fixture-secret-token';});
 assert.equal(client.getLoginState(),'disconnected');await assert.rejects(client.apps(),code('LOGIN_REQUIRED'));await client.connect();assert.equal(tokens,1);assert.equal(client.getLoginState(),'connected');assert.equal((await client.apps())[0].id,'AppA');
 for(const encoded of [JSON.stringify(client),JSON.stringify({...client}),inspect(client)])for(const secret of [key,'fixture-secret-token'])assert.equal(encoded.includes(secret),false);
 assert.deepEqual(requests.map(r=>new URL(r.url).pathname),['/api/v1/console/auth/config','/api/v1/console/auth/exchange','/api/v1/console/apps']);await client.disconnect();assert.equal(client.getLoginState(),'disconnected');assert.ok(requests.at(-1).url.endsWith('/console/session'));
});
test('configuration mismatches and unknown scopes never call host authorization',async t=>{
 let tokens=0,value=config();const {client}=fixture(t,()=>json(value),async()=>{tokens++;return 'fixture-secret-token';});
 for(const changed of [{issuer:'https://evil.example/oidc'},{resource:baseUrl},{clientId:'Other'},{tokenEndpoint:'https://evil.example/token'},{scope:'connect arbitrary'},{scope:'openid'},{scope:'connect connect'}]){value={...config(),...changed};await assert.rejects(client.connect(),code('CONFIGURATION_MISMATCH'));assert.equal(tokens,0);}
});
test('host and transport errors expose fixed errors only',async t=>{
 const {client}=fixture(t,normal,async()=>{const e=new CyberGridError('NETWORK_ERROR');e.message='fixture-secret-host-error';throw e;});await assert.rejects(client.connect(),code('AUTHENTICATION_FAILED'));assert.equal(client.getLoginState(),'disconnected');
});
test('single-flight connect and caller cancellation keep the other waiter alive',async t=>{
 const token=deferred(),entered=deferred();let count=0;const {client}=fixture(t,normal,async()=>{count++;entered.resolve();return token.promise;});const cancel=new AbortController();const first=client.connect({signal:cancel.signal}),second=client.connect();await entered.promise;cancel.abort();await assert.rejects(first,code('CANCELLED'));token.resolve('fixture-secret-token');await second;assert.equal(count,1);assert.equal(client.getLoginState(),'connected');await client.disconnect();
});
test('disconnect fences late host tokens and late exchange receipts; old receipt cleanup is console-only',async t=>{
 const late=deferred(),entered=deferred();const {client,requests}=fixture(t,(path)=>{if(path==='/console/auth/exchange'){entered.resolve();return late.promise;}return normal(path);});const connecting=client.connect();await entered.promise;const rejection=assert.rejects(connecting,code('CANCELLED'));await client.disconnect();await rejection;late.resolve(json(receipt()));await turn();await turn();assert.equal(client.getLoginState(),'disconnected');assert.equal(requests.filter(r=>r.init.method==='DELETE').length,1);assert.ok(requests.at(-1).url.endsWith('/console/session'));
});
test('disconnect during host callback cannot initiate a delayed exchange',async t=>{
 const late=deferred(),entered=deferred();const {client,requests}=fixture(t,normal,async()=>{entered.resolve();return late.promise;});const connecting=client.connect();await entered.promise;const rejection=assert.rejects(connecting,code('CANCELLED'));await client.disconnect();await rejection;late.resolve('fixture-secret-token');await turn();assert.equal(requests.length,1);assert.equal(client.getLoginState(),'disconnected');
});
test('unknown exchange outcomes reuse the epoch-bound request ID on retry',async t=>{
 const bodies=[];let fail=true;const {client}=fixture(t,(path,init)=>{if(path==='/console/auth/exchange'){bodies.push(JSON.parse(init.body));if(fail){fail=false;throw Error('fixture-secret-network');}return json(receipt());}return normal(path);});await assert.rejects(client.connect(),code('NETWORK_ERROR'));await client.connect();assert.equal(bodies.length,2);assert.equal(bodies[0].requestId,bodies[1].requestId);assert.equal(bodies[0].coordinatorEpoch,epoch);await client.disconnect();
});
test('epoch recovery is single flight and identity changes require explicit disconnect',async t=>{
 let currentEpoch=epoch,subject='user-a',exchanges=0;const {client}=fixture(t,(path)=>{if(path.startsWith('/console/auth/config?'))return json(config(currentEpoch),currentEpoch);if(path==='/console/auth/exchange'){exchanges++;return json(receipt(currentEpoch,subject),currentEpoch);}if(path==='/console/apps'&&currentEpoch!==epoch)return json({error:{code:'COORDINATOR_RESET',message:'fixture-secret-detail'}},currentEpoch,409);return normal(path);});await client.connect();currentEpoch=otherEpoch;subject='user-b';const results=await Promise.allSettled([client.apps(),client.apps()]);for(const r of results){assert.equal(r.status,'rejected');assert.equal(r.reason.code,'IDENTITY_CHANGED');}assert.equal(exchanges,2);assert.equal(client.getLoginState(),'disconnected');await client.disconnect();await client.connect();assert.equal(client.getLoginState(),'connected');await client.disconnect();
});
test('port lookup never falls back to device tunnel and separates not-found from offline',async t=>{
 let online=true;const devices=()=>({devices:[{id:'device',name:'Peer',online,lastSeen:new Date().toISOString(),tunnelUrl:'https://encrypted.trycloudflare.com',iroh:{endpointId:'a'.repeat(64),addresses:['192.0.2.1:1']},ports:[{id:'web',port:8080,type:'http',transport:'tcp',tunnelUrl:'https://public-port.trycloudflare.com'},{id:'unannounced',port:8081,type:'http',transport:'tcp'}]}]});const {client}=fixture(t,path=>path.startsWith('/console/devices?')?json(devices()):normal(path));await client.connect();assert.equal(await client.getPortTunnelUrl('AppA','device','web'),'https://public-port.trycloudflare.com');assert.equal(await client.getPortTunnelUrl('AppA','device','unannounced'),null);assert.equal(Object.hasOwn((await client.devices('AppA'))[0],'iroh'),false);assert.equal(Object.hasOwn((await client.devices('AppA'))[0],'tunnelUrl'),false);online=false;assert.equal(await client.getPortTunnelUrl('AppA','device','web'),null);await assert.rejects(client.getPortTunnelUrl('AppA','device','unknown'),code('NOT_FOUND'));await assert.rejects(client.getPortTunnelUrl('AppA','unknown','web'),code('NOT_FOUND'));await client.disconnect();
});
test('directory capacity is a fixed public error; oversized or redirect failures disclose no server text',async t=>{
 let mode='capacity';const {client}=fixture(t,path=>{if(path==='/console/apps'){if(mode==='capacity')return json({error:{code:'DIRECTORY_TOO_LARGE',message:'fixture-secret-detail'}},epoch,413);if(mode==='huge')return new Response(' '.repeat(2*1024*1024+1),{headers:{'x-cybergrid-epoch':epoch}});throw Error('fixture-secret-redirect');}return normal(path);});await client.connect();await assert.rejects(client.apps(),code('DIRECTORY_TOO_LARGE'));mode='huge';await assert.rejects(client.apps(),code('INVALID_RESPONSE'));mode='redirect';await assert.rejects(client.apps(),code('NETWORK_ERROR'));await client.disconnect();
});
test('uncertain receipts and unconfirmed cleanup debts remain bounded across disconnect generations',async t=>{
 let exchanges=0,mode='unknown';const {client}=fixture(t,(path)=>{if(path==='/console/auth/exchange'){exchanges++;if(mode==='unknown')throw Error('fixture-secret-lost-response');return json(receipt(epoch,'user-a',String.fromCharCode(97+exchanges).repeat(43)));}if(path==='/console/session')throw Error('fixture-secret-delete');return normal(path);});
 for(let i=0;i<8;i++){if(i>=4)mode='known';if(i<4)await assert.rejects(client.connect(),code('NETWORK_ERROR'));else await client.connect();await client.disconnect();}assert.equal(exchanges,8);await assert.rejects(client.connect(),code('SESSION_UNAVAILABLE'));assert.equal(exchanges,8);assert.equal(client.getLoginState(),'disconnected');
});
test('a stalled response body is bounded and cannot create unlimited orphan sessions',async t=>{
 let exchanges=0;const {client}=fixture(t,path=>{if(path==='/console/auth/exchange'){exchanges++;return new Response(new ReadableStream({start(c){c.enqueue(new TextEncoder().encode('{'));}}),{headers:{'x-cybergrid-epoch':epoch}});}return normal(path);},undefined,100);
 for(let i=0;i<8;i++){await assert.rejects(client.connect(),code('TIMEOUT'));await client.disconnect();}await assert.rejects(client.connect(),code('SESSION_UNAVAILABLE'));assert.equal(exchanges,8);
});

test('all cancelled waiters followed by retry still own and clean a late old-user receipt',async t=>{
 const late=deferred(),entered=deferred();let exchanges=0;const deletes=[];
 const {client}=fixture(t,(path,init)=>{if(path==='/console/auth/exchange'){if(++exchanges===1){entered.resolve();return late.promise;}return json(receipt(epoch,'user-b','b'.repeat(43)));}if(path==='/console/session'){deletes.push(init.headers.authorization);return json({ok:true});}return normal(path);});
 const firstAbort=new AbortController(),secondAbort=new AbortController();const first=client.connect({signal:firstAbort.signal}),second=client.connect({signal:secondAbort.signal});await entered.promise;const rejections=Promise.all([assert.rejects(first,code('CANCELLED')),assert.rejects(second,code('CANCELLED'))]);firstAbort.abort();secondAbort.abort();await rejections;await client.connect();late.resolve(json(receipt(epoch,'user-a')));await turn();await turn();assert.equal(client.getLoginState(),'connected');assert.deepEqual(deletes,['Bearer '+key]);await client.disconnect();assert.deepEqual(deletes,['Bearer '+key,'Bearer '+'b'.repeat(43)]);
});

test('a remembered cancelled receipt is cleaned when the retry receives a different host identity',async t=>{
 const late=deferred(),entered=deferred();let exchanges=0;const deletes=[];const {client}=fixture(t,(path,init)=>{if(path==='/console/auth/exchange'){if(++exchanges===1){entered.resolve();return late.promise;}return json(receipt(epoch,'user-b','b'.repeat(43)));}if(path==='/console/session'){deletes.push(init.headers.authorization);return json({ok:true});}return normal(path);});
 const controller=new AbortController(),first=client.connect({signal:controller.signal});await entered.promise;const rejection=assert.rejects(first,code('CANCELLED'));controller.abort();await rejection;late.resolve(json(receipt(epoch,'user-a')));await turn();await turn();await client.connect();await turn();assert.deepEqual(deletes,['Bearer '+key]);await client.disconnect();assert.deepEqual(deletes,['Bearer '+key,'Bearer '+'b'.repeat(43)]);
});
