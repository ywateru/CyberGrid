import {test} from 'node:test';
import {once} from 'node:events';
import assert from 'node:assert/strict';
import {request as httpRequest,type IncomingMessage} from 'node:http';
import {createNodeServer} from '../src/node-server.js';
import {effectiveOrigin} from '../src/http.js';
import {coordinatorAuthConfiguration,DEFAULT_RESOURCE,DEFAULT_CALLBACK} from '../src/deployment-config.js';
import {MemoryStore} from '../src/memory.js';
import {createService} from '../src/service.js';
import {LogtoIdentity} from '../src/identity.js';
import {FixtureDirectory,A,SPA} from './fixtures.js';
import {createLocalJWKSet,generateKeyPair,exportJWK,SignJWT} from 'jose';
const env={LOGTO_ISSUER:'https://tenant.example/oidc',LOGTO_BROWSER_CLIENT_ID:SPA};
const raw=(headers:string[],encrypted=false)=>({rawHeaders:headers,socket:{encrypted}} as unknown as IncomingMessage);
test('optional public origin has three fixed resource priorities and no audience dependence on requests',()=>{
 assert.equal(coordinatorAuthConfiguration(env).resource,DEFAULT_RESOURCE);assert.equal(coordinatorAuthConfiguration({...env,CYBERGRID_PUBLIC_ORIGIN:''}).resource,DEFAULT_RESOURCE);
 assert.equal(coordinatorAuthConfiguration({...env,CYBERGRID_PUBLIC_ORIGIN:'https://fixed.example'}).resource,'https://fixed.example/api');
 for(const origin of [undefined,'','https://fixed.example'])assert.equal(coordinatorAuthConfiguration({...env,CYBERGRID_PUBLIC_ORIGIN:origin,LOGTO_RESOURCE:'https://identity.example/fixed'}).resource,'https://identity.example/fixed');
 assert.equal(coordinatorAuthConfiguration(env).redirectUri,DEFAULT_CALLBACK);
 assert.throws(()=>coordinatorAuthConfiguration({...env,LOGTO_RESOURCE:'http://bad.example/api'}));
 for(const redirect of ['http://site.example/callback','/custom','https://site.example/api/x','https://site.example/admin/x','https://site.example/healthz','https://site.example/','https://user@site.example/callback','https://site.example/callback?x'])assert.throws(()=>coordinatorAuthConfiguration({...env,CYBERGRID_CONSOLE_REDIRECT_URI:redirect}),redirect);
 assert.equal(coordinatorAuthConfiguration({...env,CYBERGRID_CONSOLE_REDIRECT_URI:'https://site.example/auth/return'}).redirectUri,'https://site.example/auth/return');
});
test('strict request origin handles direct HTTP/TLS and single proxy headers without caching or client-IP use',()=>{
 assert.equal(effectiveOrigin(raw(['Host','localhost:7400'])),'http://localhost:7400');assert.equal(effectiveOrigin(raw(['Host','example.test'],true)),'https://example.test');
 assert.equal(effectiveOrigin(raw(['Host','internal:7400','X-Forwarded-Host','GRID.example:443','X-Forwarded-Proto','https','X-Forwarded-For','spoof','Forwarded','host=ignored;proto=ftp'])),'https://grid.example');
 assert.equal(effectiveOrigin(raw(['Host','[::1]:7400'])),'http://[::1]:7400');assert.equal(effectiveOrigin(raw(['Host','local','X-Forwarded-Proto','https'])),'https://local');
 const bad=[[],['Host','one','host','two'],['Host','one,two'],['Host','one','X-Forwarded-Host','one','x-forwarded-host','two'],['Host','one','X-Forwarded-Proto','https','x-forwarded-proto','https'],['Host','one','X-Forwarded-Host','one,two'],['Host','one','X-Forwarded-Proto','http,https'],['Host','one','X-Forwarded-Proto','ftp'],['Host','one','X-Forwarded-Proto','HTTPS'],...['user@host','host/path','host?x','host#x','host\\evil','host:0','host:65536','host:abc','host:','host:000080','bad host','host\n','[not-ip]','host..name','-host','host%2eexample'].map(h=>['Host',h])];
 for(const headers of bad)assert.throws(()=>effectiveOrigin(raw(headers)),JSON.stringify(headers));
 assert.equal(effectiveOrigin(raw(['Host','attacker','X-Forwarded-Host','other','X-Forwarded-Proto','http']),'https://fixed.example'),'https://fixed.example');
});
async function listener(t:any,configuration:ReturnType<typeof coordinatorAuthConfiguration>,publicOrigin?:string){
 const store=new MemoryStore(),identity=new LogtoIdentity(configuration);const handle=createService({store,identity,directory:new FixtureDirectory(),browserClientId:SPA});const server=createNodeServer(handle,{publicOrigin,issuer:configuration.issuer,callbackUri:configuration.redirectUri});
 await new Promise<void>(r=>server.listen(0,'127.0.0.1',r));const port=(server.address() as {port:number}).port;
 t.after(async()=>{store.close();server.closeAllConnections();await new Promise<void>(r=>server.close(()=>r()));});
 const request=(host:string,headers:Record<string,string>={},path='/api/v1/console/auth/config')=>new Promise<{status:number;body:any}>(r=>{const q=httpRequest({host:'127.0.0.1',port,path,headers:{host,...headers}},res=>{let text='';res.on('data',c=>text+=c);res.on('end',()=>r({status:res.statusCode!,body:res.headers['content-type']?.includes('application/json')?JSON.parse(text):text}));});q.end();});return {request};
}
test('real Node config responses project absolute per-request callbacks and keep resource immutable for HTTP and HTTPS proxy hosts',async t=>{
 const configuration=coordinatorAuthConfiguration(env),f=await listener(t,configuration);
 for(const [host,headers,origin] of [['local.example:7400',{},'http://local.example:7400'],['internal:7400',{'x-forwarded-host':'public.example','x-forwarded-proto':'https'},'https://public.example'],['second.example',{},'http://second.example']] as [string,Record<string,string>,string][]){
  for(const path of ['/api/v1/console/auth/config','/api/v1/auth/config?appId='+A]){const r=await f.request(host,headers,path);assert.equal(r.status,200);assert.equal(r.body.redirectUri,origin+DEFAULT_CALLBACK);assert.equal(r.body.resource,DEFAULT_RESOURCE);}
 }
 assert.equal(configuration.redirectUri,DEFAULT_CALLBACK);assert.equal(configuration.resource,DEFAULT_RESOURCE);
 assert.equal((await f.request('internal',{'x-forwarded-host':'public.example','x-forwarded-proto':'https',origin:'https://public.example'})).status,200);
 assert.equal((await f.request('internal',{'x-forwarded-host':'public.example','x-forwarded-proto':'https',origin:'https://evil.example'})).status,403);
 assert.equal((await f.request('one',{'x-forwarded-proto':'https,http'})).status,400);
});
test('configured public origin pins callbacks while unset public origin validates explicit callback per request',async t=>{
 const configured=coordinatorAuthConfiguration({...env,CYBERGRID_PUBLIC_ORIGIN:'https://fixed.example'}),fixed=await listener(t,configured,'https://fixed.example');const r=await fixed.request('evil.example',{'x-forwarded-host':'evil.example','x-forwarded-proto':'http'});assert.equal(r.status,200);assert.equal(r.body.redirectUri,'https://fixed.example'+DEFAULT_CALLBACK);assert.equal(r.body.resource,'https://fixed.example/api');
 const custom=coordinatorAuthConfiguration({...env,CYBERGRID_CONSOLE_REDIRECT_URI:'https://custom.example/return'}),f=await listener(t,custom);
 assert.equal((await f.request('internal',{'x-forwarded-host':'custom.example','x-forwarded-proto':'https'})).body.redirectUri,'https://custom.example/return');
 assert.equal((await f.request('other.example')).status,400);assert.equal((await f.request('other.example',{},'/return?code=fixture')).status,400);
 const callback=await f.request('internal',{'x-forwarded-host':'custom.example','x-forwarded-proto':'https'},'/return?code=fixture&state=value');assert.equal(callback.status,200);assert.match(callback.body,/CyberGrid/);
});
test('real ES384 verifier retains fixed resource; a request-host audience cannot replace it',async()=>{
 const config=coordinatorAuthConfiguration(env),pair=await generateKeyPair('ES384'),jwk={...await exportJWK(pair.publicKey),kid:'origin-test',alg:'ES384'};const identity=new LogtoIdentity(config,['ES384'],createLocalJWKSet({keys:[jwk]}));
 const token=(audience:string)=>new SignJWT({client_id:A,scope:'connect'}).setSubject('fixture-user').setIssuer(config.issuer).setAudience(audience).setIssuedAt().setExpirationTime('1m').setProtectedHeader({alg:'ES384',kid:jwk.kid,typ:'JWT'}).sign(pair.privateKey);
 assert.equal((await identity.verify(await token(DEFAULT_RESOURCE))).clientId,A);await assert.rejects(()=>token('https://request.example/api').then(value=>identity.verify(value)),(e:any)=>e.code==='INVALID_IDENTITY');
});

test('actual Node entrypoint starts without PUBLIC_ORIGIN and serves native HTTP plus HTTPS-forwarded config',{timeout:10000},async t=>{
 const {spawn}=await import('node:child_process'),{mkdtemp,rm}=await import('node:fs/promises'),{tmpdir}=await import('node:os'),{join}=await import('node:path'),{createServer}=await import('node:net');
 const dir=await mkdtemp(join(tmpdir(),'cybergrid-optional-origin-')),probe=createServer();await new Promise<void>(r=>probe.listen(0,'127.0.0.1',r));const port=(probe.address() as {port:number}).port;await new Promise<void>(r=>probe.close(()=>r()));
 const childEnv:NodeJS.ProcessEnv={...process.env,...env,NODE_ENV:'test',CYBERGRID_BIND:'127.0.0.1',PORT:String(port),CYBERGRID_OWNER_LOCK:join(dir,'lease'),LOGTO_MANAGEMENT_CLIENT_ID:'FixtureDirectory',LOGTO_MANAGEMENT_CLIENT_SECRET:'fixture-not-used'};delete childEnv.CYBERGRID_PUBLIC_ORIGIN;delete childEnv.LOGTO_RESOURCE;
 const child=spawn(process.execPath,['--import','tsx','src/main.ts'],{env:childEnv,stdio:['ignore','pipe','pipe']});
 t.after(async()=>{if(child.exitCode===null&&child.signalCode===null){child.kill('SIGTERM');await once(child,'exit');}await rm(dir,{recursive:true,force:true});});
 child.stderr.resume();await new Promise<void>((r,reject)=>{let out='';child.stdout.on('data',c=>{out+=String(c);if(out.includes('Node service listening'))r();});child.once('exit',()=>reject(Error('entrypoint exited before ready')));});
 const base='http://127.0.0.1:'+port;assert.equal(await(await fetch(base+'/healthz')).text(),'ok\n');
 const local=await(await fetch(base+'/api/v1/console/auth/config')).json() as any;assert.equal(local.resource,DEFAULT_RESOURCE);assert.equal(local.redirectUri,base+DEFAULT_CALLBACK);
 const external=await(await fetch(base+'/api/v1/console/auth/config',{headers:{'x-forwarded-host':'external.example','x-forwarded-proto':'https'}})).json() as any;assert.equal(external.resource,DEFAULT_RESOURCE);assert.equal(external.redirectUri,'https://external.example'+DEFAULT_CALLBACK);
});
