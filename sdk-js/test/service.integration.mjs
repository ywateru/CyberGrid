import {test} from 'node:test';
import assert from 'node:assert/strict';
import {randomBytes,randomUUID} from 'node:crypto';
import {CyberGridClient,CyberGridError} from '../dist/index.js';
import {createService} from '../../server/src/service.ts';
import {MemoryStore} from '../../server/src/memory.ts';
import {createNodeServer} from '../../server/src/node-server.ts';
import {FixtureIdentity,FixtureDirectory,A,SPA} from '../../server/test/fixtures.ts';

test('built ESM interoperates with real Node HTTP, CORS, MemoryStore receipts, port snapshots and epoch recovery',async t=>{
 const identity=new FixtureIdentity(),directory=new FixtureDirectory(),user=(await identity.register('js@example.test')).user;
 let store=new MemoryStore(),clock=Date.now();const frontend='https://host-frontend.example';
 const makeHandle=()=>createService({store,identity,directory,browserClientId:SPA,browserOrigins:new Set([frontend]),now:()=>clock,authRateLimit:100});let handle=makeHandle();
 const server=createNodeServer((request,ip)=>handle(request,ip),{issuer:'https://fixture.invalid/oidc',callbackUri:'/console/callback/'});
 await new Promise(resolve=>server.listen(0,'127.0.0.1',resolve));const origin='http://127.0.0.1:'+server.address().port;
 t.after(async()=>{store.close();server.closeAllConnections();await new Promise(resolve=>server.close(resolve));});
 const original=globalThis.fetch;let exchanges=0,dropReceipt=true;const paths=[];
 globalThis.fetch=async(url,init)=>{assert.equal(new URL(url).origin,origin);paths.push(new URL(url).pathname);const headers=new Headers(init?.headers);headers.set('origin',frontend);const response=await original(url,{...init,headers});assert.equal(response.headers.get('access-control-allow-origin'),frontend);if(url.endsWith('/console/auth/exchange')){exchanges++;if(dropReceipt){dropReceipt=false;await response.arrayBuffer();throw Error('Simulated lost console receipt');}}return response;};t.after(()=>{globalThis.fetch=original;});
 const publish=async()=>{const r=store.register(user.id,A,'JS fixture peer',randomBytes(32).toString('base64url'),randomUUID(),clock,1);await store.publish(r.session,{ports:[{id:'web',port:8080,type:'http',transport:'tcp',tunnelUrl:'https://fixture-business.trycloudflare.com'},{id:'private',port:8081,type:'tcp',transport:'tcp'}],tunnelUrl:'https://fixture-encrypted.trycloudflare.com',certificateFingerprint:'b'.repeat(64),iroh:{endpointId:'a'.repeat(64),addresses:['192.0.2.1:1234']}},r.session.incarnation,1,clock);return r;};
 const native=await publish();let tokenCalls=0;
 const client=new CyberGridClient({baseUrl:origin,issuer:'https://fixture.invalid/oidc',clientId:SPA,resource:'https://cybergrid.aixgg.com/api',getAccessToken:async({scopes})=>{assert.deepEqual(scopes,['connect']);tokenCalls++;return identity.tokenFor('js@example.test',SPA);}});
 await assert.rejects(client.connect(),e=>e instanceof CyberGridError&&e.code==='NETWORK_ERROR');await client.connect();assert.equal(exchanges,2);assert.equal(tokenCalls,1,'unknown result replays the original token without asking the grant owner again');assert.equal(store.adminOverview(clock,{page:1,pageSize:100}).totals.sessions,2,'unknown receipt retry reuses the single console session');
 assert.deepEqual((await client.apps()).map(a=>a.id),directory.apps.filter(a=>a.type==='Native').map(a=>a.id));const peer=(await client.devices(A))[0];assert.equal(peer.id,native.session.deviceId);assert.equal(Object.hasOwn(peer,'iroh'),false);assert.equal(Object.hasOwn(peer,'tunnelUrl'),false);assert.equal(await client.getPortTunnelUrl(A,peer.id,'web'),'https://fixture-business.trycloudflare.com');assert.equal(await client.getPortTunnelUrl(A,peer.id,'private'),null);
 clock+=66000;assert.equal(await client.getPortTunnelUrl(A,peer.id,'web'),null);await assert.rejects(client.getPortTunnelUrl(A,peer.id,'missing'),e=>e.code==='NOT_FOUND');
 store.close();store=new MemoryStore();handle=makeHandle();await publish();const [apps,devices]=await Promise.all([client.apps(),client.devices(A)]);assert.equal(apps.length,2);assert.equal(devices.length,1);assert.equal(tokenCalls,2,'one initial token and one shared recovery token; unknown receipt retry asks for none');assert.equal(store.adminOverview(clock,{page:1,pageSize:100}).totals.sessions,2);
 await client.disconnect();assert.equal(store.adminOverview(clock,{page:1,pageSize:100}).totals.sessions,1);assert.equal(client.getLoginState(),'disconnected');assert.ok(paths.every(path=>path.startsWith('/api/v1/console/')));
 const deviceBearer=store.register(user.id,A,'native',randomBytes(32).toString('base64url'),randomUUID(),clock,1);
 const forbidden=await original(origin+'/api/v1/console/session',{method:'DELETE',headers:{origin:frontend,authorization:'Bearer '+deviceBearer.sessionKey}});assert.equal(forbidden.status,403);assert.ok(await store.session(deviceBearer.session.hash));
});
