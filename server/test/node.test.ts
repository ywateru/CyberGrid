import {test} from 'node:test';
import assert from 'node:assert/strict';
import {request as httpRequest} from 'node:http';
import {mkdtemp,mkdir,writeFile,symlink,rm} from 'node:fs/promises';
import {tmpdir} from 'node:os';
import {join} from 'node:path';
import {createNodeServer} from '../src/node-server.js';
import {publicOrigin} from '../src/deployment-config.js';
import {createService} from '../src/service.js';
import {MemoryStore} from '../src/memory.js';
import {FixtureIdentity,FixtureDirectory,SPA,A} from './fixtures.js';
const origin='https://grid.example';
async function fixture(t:any){
 const root=await mkdtemp(join(tmpdir(),'cybergrid-node-'));for(const dir of ['console','docs','admin']){await mkdir(join(root,dir));await writeFile(join(root,dir,'index.html'),'<html>'+dir+'</html>');}await writeFile(join(root,'index.html'),'<html>home</html>');await writeFile(join(root,'site.js'),'console.log(1)');
 const store=new MemoryStore(),identity=new FixtureIdentity();await identity.register('fixture@example.test');
 const api=createService({store,identity,directory:new FixtureDirectory(),browserClientId:SPA,adminKey:'a'.repeat(48)});
 let seenURL='',seenIP='';const server=createNodeServer((r,ip)=>{seenURL=r.url;seenIP=ip??'';return api(r,ip);},{publicOrigin:origin,issuer:'https://tenant.example/oidc',callbackUri:origin+'/auth/return',publicRoot:root});
 await new Promise<void>(r=>server.listen(0,'127.0.0.1',r));const port=(server.address() as {port:number}).port;
 t.after(async()=>{store.close();server.closeAllConnections();await new Promise<void>(r=>server.close(()=>r()));await rm(root,{recursive:true,force:true});});
 const request=(path:string,options:{method?:string;headers?:Record<string,string>;body?:string}={})=>new Promise<{status:number;headers:any;body:string}>((resolve,reject)=>{const req=httpRequest({hostname:'127.0.0.1',port,path,method:options.method??'GET',headers:{host:'attacker.example',...options.headers}},res=>{let body='';res.on('data',c=>body+=c);res.on('end',()=>resolve({status:res.statusCode!,headers:res.headers,body}));});req.on('error',reject);req.end(options.body);});
 return {root,store,identity,request,port,seen:()=>({url:seenURL,ip:seenIP})};
}
test('one Node listener serves website, health and direct same-store API/admin with canonical URL and socket IP',async t=>{
 const f=await fixture(t);for(const path of ['/','/docs/','/admin/']){const r=await f.request(path);assert.equal(r.status,200);assert.match(r.headers['content-type'],/^text\/html/);assert.match(r.headers['content-security-policy'],/connect-src 'self' https:\/\/tenant.example;/);}
 const health=await f.request('/healthz');assert.equal(health.body,'ok\n');assert.equal(health.headers['cache-control'],'no-store');assert.equal((await f.request('/healthz',{method:'HEAD'})).body,'');
 const login=await f.request('/api/v1/auth/exchange',{method:'POST',headers:{'content-type':'application/json',origin,forwarded:'for=evil;host=evil','x-forwarded-for':'spoof','x-cybergrid-client-ip':'spoof'},body:JSON.stringify({accessToken:f.identity.tokenFor('fixture@example.test'),appId:A,deviceName:'node-fixture'})});assert.equal(login.status,200);
 assert.equal(new URL(f.seen().url).origin,origin);assert.equal(f.seen().ip,'127.0.0.1');
 const session=JSON.parse(login.body);assert(f.store.epoch);assert.equal(login.headers['x-cybergrid-epoch'],f.store.epoch);
 const admin=await f.request('/api/v1/admin/overview',{headers:{authorization:'Bearer '+'a'.repeat(48)}});assert.equal(admin.status,200);assert.equal(JSON.parse(admin.body).coordinator.epoch,f.store.epoch);
 assert.equal((await f.request('/api/v1/devices',{headers:{authorization:'Bearer '+session.sessionKey,origin:'https://attacker.example','x-forwarded-host':'grid.example'}})).status,403);
});
test('GET/HEAD static and custom callback retain code query; traversal, symlinks, secrets and unsupported methods fail closed',async t=>{
 const f=await fixture(t);for(const path of ['/auth/return?code=fixture&state=state','/console/callback/?code=fixture','/console/callback']){const r=await f.request(path);assert.equal(r.status,200);assert.equal(r.body,'<html>console</html>');assert.equal(r.headers.location,undefined);}
 const get=await f.request('/site.js'),head=await f.request('/site.js',{method:'HEAD'});assert.equal(head.status,200);assert.equal(head.body,'');assert.equal(head.headers['content-length'],String(Buffer.byteLength(get.body)));assert.match(head.headers['content-type'],/^text\/javascript/);
 await writeFile(join(f.root,'.env'),'private');await symlink('/etc/passwd',join(f.root,'leak'));await symlink('/etc',join(f.root,'outside'));
 for(const path of ['/../.env','/%2e%2e/.env','/%252e%252e/.env','/console%2f..%2f.env','//evil.example/','.env','/bad%00','/bad%zz','/outside/passwd','/leak','/.env','/downloads/private'])assert.notEqual((await f.request(path)).status,200,path);
 assert.equal((await f.request('/',{method:'POST',body:'{}'})).status,405);assert.equal((await f.request('/healthz',{method:'POST',body:'{}'})).status,405);
});
test('public origin is required HTTPS, independent of localhost listener and forwarding headers',()=>{
 for(const value of [undefined,'http://localhost','https://example/path','https://user@example','https://example?x',' https://example'])assert.throws(()=>publicOrigin(value));assert.equal(publicOrigin('https://GRID.example:443/'),'https://grid.example');
});

test('real HTTP long-poll disconnect and coordinator close release same-store waiters',async t=>{
 const f=await fixture(t);const login=await f.request('/api/v1/auth/exchange',{method:'POST',headers:{'content-type':'application/json'},body:JSON.stringify({accessToken:f.identity.tokenFor('fixture@example.test'),appId:A,deviceName:'poller'})});
 const session=JSON.parse(login.body),headers={authorization:'Bearer '+session.sessionKey,'content-type':'application/json'};
 assert.equal((await f.request('/api/v1/devices/self',{method:'PUT',headers,body:JSON.stringify({ports:[],certificateFingerprint:'b'.repeat(64)})})).status,200);
 const count=async()=>JSON.parse((await f.request('/api/v1/admin/overview',{headers:{authorization:'Bearer '+'a'.repeat(48)}})).body).totals.waiters;
 const waitCount=async(n:number)=>{for(let i=0;i<50;i++){if(await count()===n)return;await new Promise(r=>setTimeout(r,10));}assert.fail('waiter count did not become '+n);};
 const req=httpRequest({host:'127.0.0.1',port:f.port,path:'/api/v1/signals?wait=15',headers},r=>r.resume());req.on('error',()=>{});req.end();await waitCount(1);req.destroy();await waitCount(0);
 const pending=f.request('/api/v1/signals?wait=15',{headers});await waitCount(1);f.store.close();const response=await pending;assert.equal(response.status,503);
});
