import {test} from 'node:test';
import assert from 'node:assert/strict';
import {browserOrigins} from '../src/console-cors.js';
import {createService} from '../src/service.js';
import {MemoryStore} from '../src/memory.js';
import {A,SPA,FixtureIdentity,FixtureDirectory} from './fixtures.js';

const ORIGIN='https://frontend.example';
async function fixture(){
 const store=new MemoryStore(),identity=new FixtureIdentity();await identity.register('one@example.test');
 const handle=createService({store,identity,directory:new FixtureDirectory(),browserClientId:SPA,browserOrigins:browserOrigins(ORIGIN)});
 const request=(path:string,method='GET',body?:unknown,bearer?:string,headers:Record<string,string>={origin:ORIGIN,'sec-fetch-site':'cross-site'})=>handle(new Request('https://grid.example/api/v1'+path,{method,headers:{...headers,...(body===undefined?{}:{'content-type':'application/json'}),...(bearer?{authorization:'Bearer '+bearer}:{})},body:body===undefined?undefined:JSON.stringify(body)}));
 return {store,identity,request};
}
test('browser origins are explicit canonical HTTPS or local HTTP origins, with bounded input',()=>{
 assert.equal(browserOrigins(undefined).size,0);assert.equal(browserOrigins('').size,0);
 assert.deepEqual([...browserOrigins('https://frontend.example, http://localhost:3000,http://127.0.0.1:4000,http://[::1]:5000')],[ORIGIN,'http://localhost:3000','http://127.0.0.1:4000','http://[::1]:5000']);
 for(const value of ['*','null','http://frontend.example','https://user@frontend.example','https://frontend.example/','https://frontend.example/path','https://frontend.example?query=1','https://frontend.example#fragment',ORIGIN+',',ORIGIN+','+ORIGIN,Array.from({length:17},(_,i)=>`https://frontend${i}.example`).join(','),'x'.repeat(4097)])assert.throws(()=>browserOrigins(value));
});
test('console CORS reflects only exact approved origins, exposes epoch, and varies every console response',async t=>{
 const f=await fixture();t.after(()=>f.store.close());
 const allowed=await f.request('/console/auth/config');assert.equal(allowed.status,200);assert.equal(allowed.headers.get('access-control-allow-origin'),ORIGIN);assert.equal(allowed.headers.get('access-control-allow-credentials'),null);assert.match(allowed.headers.get('access-control-expose-headers')!,/X-CyberGrid-Epoch/);assert.match(allowed.headers.get('vary')!,/Origin/);
 for(const origin of ['https://frontend.example.evil','https://other.example','null',ORIGIN+', https://evil.example']){const r=await f.request('/console/auth/config','GET',undefined,undefined,{origin});assert.equal(r.status,403);assert.equal(r.headers.get('access-control-allow-origin'),null);}
 const same=await f.request('/console/auth/config','GET',undefined,undefined,{origin:'https://grid.example'});assert.equal(same.status,200);assert.equal(same.headers.get('access-control-allow-origin'),null);assert.match(same.headers.get('vary')!,/Origin/);
 const unauth=await f.request('/console/apps');assert.equal(unauth.status,401);assert.equal(unauth.headers.get('access-control-allow-origin'),ORIGIN);
});
test('OPTIONS allows only console route/method/header tuples and never device/admin/signal routes',async t=>{
 const f=await fixture();t.after(()=>f.store.close());
 for(const [path,method] of [['/console/auth/config','GET'],['/console/auth/exchange','POST'],['/console/apps','GET'],['/console/devices','GET'],['/console/session','DELETE']]){
  const r=await f.request(path!,'OPTIONS',undefined,undefined,{origin:ORIGIN,'access-control-request-method':method!,'access-control-request-headers':'Authorization, Content-Type, X-CyberGrid-Epoch'});assert.equal(r.status,204);assert.equal(r.headers.get('access-control-allow-origin'),ORIGIN);assert.equal(r.headers.get('access-control-allow-methods'),method);assert.equal(r.headers.get('access-control-allow-credentials'),null);assert.match(r.headers.get('vary')!,/Access-Control-Request-Headers/);
 }
 for(const path of ['/apps','/devices','/devices/self','/signals','/admin/overview','/auth/config','/auth/exchange','/session']){
  for(const method of ['GET','POST','PUT','DELETE']){const r=await f.request(path,'OPTIONS',undefined,undefined,{origin:ORIGIN,'access-control-request-method':method});assert.equal(r.status,403);assert.equal(r.headers.get('access-control-allow-origin'),null);}
 }
 const invalidHeaders:Record<string,string>[]=[{'access-control-request-method':'POST'},{'access-control-request-method':'GET','access-control-request-headers':'cookie'},{'access-control-request-method':'GET','access-control-request-headers':'authorization,authorization'},{'access-control-request-method':'GET','access-control-request-headers':''},{}];
 for(const headers of invalidHeaders){
  const r=await f.request('/console/auth/config','OPTIONS',undefined,undefined,{origin:ORIGIN,...headers});assert.equal(r.status,403);assert.equal(r.headers.get('access-control-allow-origin'),null);
 }
});
test('cross-site authority is console-only; console logout cannot revoke a device session',async t=>{
 const f=await fixture();t.after(()=>f.store.close());
 const native=await(await f.request('/auth/exchange','POST',{accessToken:f.identity.tokenFor('one@example.test',A),appId:A,deviceName:'peer'},undefined,{})).json() as any;
 const c=await(await f.request('/console/auth/exchange','POST',{accessToken:f.identity.tokenFor('one@example.test',SPA)})).json() as any;assert.equal(c.purpose,'console');
 assert.equal((await f.request('/console/apps','GET',undefined,c.sessionKey)).status,200);
 assert.equal((await f.request('/console/devices?appId='+A,'GET',undefined,c.sessionKey)).status,200);
 for(const [path,method] of [['/console/apps','GET'],['/console/session','DELETE'],['/session','DELETE'],['/devices','GET'],['/signals','POST'],['/admin/overview','GET']])assert.equal((await f.request(path!,method!,method==='POST'?{}:undefined,native.sessionKey)).status,403);
 assert.equal((await f.request('/devices','GET',undefined,native.sessionKey,{})).status,200);assert.equal((await f.request('/apps','GET',undefined,native.sessionKey,{})).status,200);
 assert.equal((await f.request('/console/session','DELETE',undefined,native.sessionKey,{})).status,403);
 const rejected=await f.request('/console/auth/exchange','POST',{accessToken:f.identity.tokenFor('one@example.test',A)});assert.equal(rejected.status,401);
 const logout=await f.request('/console/session','DELETE',undefined,c.sessionKey);assert.equal(logout.status,200);assert.equal(logout.headers.get('access-control-allow-origin'),ORIGIN);
 assert.equal((await f.request('/console/apps','GET',undefined,c.sessionKey)).status,401);assert.equal((await f.request('/devices','GET',undefined,native.sessionKey,{})).status,200);
});
