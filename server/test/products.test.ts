import {test} from 'node:test';
import assert from 'node:assert/strict';
import {inspect} from 'node:util';
import {ProductPolicy} from '../src/products.js';
import {createConfiguredCoordinator} from '../src/config.js';
import {LogtoDirectory} from '../src/directory.js';
const product={id:'Desktop',name:'Workspace',nativeClientIds:['Mobile','Desktop'],browserClientIds:['WebB','WebA']};
const config=(products:unknown[]=[product])=>JSON.stringify({version:1,products});
test('static products normalize order and expose only canonical public application DTOs',async()=>{
 const a=new ProductPolicy(config()),b=new ProductPolicy(config([{...product,nativeClientIds:['Desktop','Mobile'],browserClientIds:['WebA','WebB']}]));
 assert.match(a.version,/^[a-f0-9]{64}$/);assert.equal(a.version,b.version);assert.deepEqual(a.native('Mobile'),{id:'Desktop',name:'Workspace'});assert.equal(a.native('WebA'),undefined);assert.deepEqual(a.browser('WebA'),[{id:'Desktop',name:'Workspace'}]);assert.equal(a.browser('unknown'),undefined);
 assert.deepEqual(await a.list(),[{id:'Desktop',name:'Workspace',type:'Native',isDeviceFlow:false}]);const publicDTO=a.native('Mobile')!;publicDTO.name='changed';assert.equal(a.native('Mobile')!.name,'Workspace');
 const other={id:'Other',name:'Other',nativeClientIds:['Other'],browserClientIds:['WebA']};assert.equal(new ProductPolicy(config([product,other])).version,new ProductPolicy(config([other,product])).version);
});
test('static products reject invalid policy without leaking configuration material',()=>{
 const bad=[undefined,null,{},[],{version:2,products:[product]},{version:1,products:[]},{version:1,products:[product],extra:true},...[
 {...product,extra:true},{...product,id:'other'},{...product,name:''},{...product,name:'x'.repeat(257)},{...product,name:'bad\nname'},
 {...product,nativeClientIds:[]},{...product,browserClientIds:[]},{...product,nativeClientIds:['Desktop','Desktop']},{...product,browserClientIds:['WebA','WebA']},{...product,nativeClientIds:['Desktop','bad/id']},{...product,browserClientIds:['Desktop']},{...product,browserClientIds:['x'.repeat(22)]},{...product,browserClientIds:Array.from({length:17},(_,i)=>'Web'+i)}
 ].map(p=>({version:1,products:[p]})),{version:1,products:[product,product]},{version:1,products:[product,{id:'Other',name:'Other',nativeClientIds:['Other','Mobile'],browserClientIds:['WebC']}]}];
 for(const value of bad)assert.throws(()=>new ProductPolicy(JSON.stringify(value)??'undefined'),/Invalid CYBERGRID_PRODUCTS configuration/);
 for(const raw of ['secret-policy-marker',' '.repeat(128*1024+1),config(Array.from({length:33},(_,i)=>({id:'P'+i,name:'P',nativeClientIds:['P'+i],browserClientIds:['Web']})))])assert.throws(()=>new ProductPolicy(raw),(e:unknown)=>{assert.equal((e as Error).message,'Invalid CYBERGRID_PRODUCTS configuration');assert.ok(!inspect(e).includes('secret-policy-marker'));assert.ok(!JSON.stringify(e).includes('secret-policy-marker'));return true;});
});
test('configured static service needs no Management credentials or calls; malformed policy never falls back',()=>{
 const env={LOGTO_ISSUER:'https://fixture.invalid/oidc',LOGTO_BROWSER_CLIENT_ID:'WebA',REDIS_URL:'redis://127.0.0.1:1',CYBERGRID_PRODUCTS:config()};
 const configured=createConfiguredCoordinator(env);configured.close();
 assert.throws(()=>createConfiguredCoordinator({...env,CYBERGRID_PRODUCTS:''}),/Invalid CYBERGRID_PRODUCTS/);
 const {CYBERGRID_PRODUCTS,...dynamic}=env;assert.throws(()=>createConfiguredCoordinator(dynamic),/management credentials/);
});
test('dynamic Management origin and resource are pinned independently of token endpoint',async()=>{
 const calls:{url:string;body:string}[]=[];
 const directory=new LogtoDirectory({issuer:'https://tenant.invalid/oidc',clientId:'m2m',clientSecret:'fixture',managementOrigin:'https://management.invalid',managementResource:'https://resource.invalid',fetch:async(input,init)=>{calls.push({url:String(input),body:String(init?.body??'')});return Response.json(calls.length===1?{access_token:'fixture',expires_in:3600,scope:'all'}:[{id:'Desktop',name:'Desktop',type:'Native'}]);}});
 assert.equal((await directory.list())[0]!.id,'Desktop');assert.equal(calls[0]!.url,'https://tenant.invalid/oidc/token');assert.equal(new URLSearchParams(calls[0]!.body).get('resource'),'https://resource.invalid');assert.equal(calls[1]!.url,'https://management.invalid/api/applications?page=1&page_size=100');
 for(const managementOrigin of ['http://bad.invalid','https://bad.invalid/path','https://bad.invalid/?x=1',' https://bad.invalid','https://bad.invalid\n','secret-origin-marker'])assert.throws(()=>new LogtoDirectory({issuer:'https://tenant.invalid/oidc',clientId:'m2m',clientSecret:'fixture',managementOrigin}),e=>!inspect(e).includes('secret-origin-marker'));
});
test('MemoryStore fixture mirrors narrow dynamic legacy receipt recovery without changing bound records',async()=>{
 const {MemoryStore}=await import('../src/memory.js'),{randomBytes,randomUUID}=await import('node:crypto');const store=new MemoryStore();const key=randomBytes(32).toString('base64url'),id=randomUUID(),old=store.register('owner','Desktop','fixture',key,id,1,1),dynamic={authClientId:'Desktop',policyMode:'dynamic' as const};
 const replay=store.register('owner','Desktop','fixture',key,id,2,1,undefined,dynamic);assert.equal(replay.sessionKey,old.sessionKey);assert.deepEqual(replay.session,old.session);
 assert.throws(()=>store.register('owner','Desktop','fixture',key,id,2,1,undefined,{authClientId:'Desktop',policyMode:'static'}),(e:any)=>e.code==='REGISTRATION_ID_CONFLICT');
 assert.throws(()=>store.register('owner','Desktop','fixture',key,id,2,1,undefined,{authClientId:'Other',policyMode:'dynamic'}),(e:any)=>e.code==='REGISTRATION_ID_CONFLICT');
 const consoleId=randomUUID(),consoleOld=store.register('owner',null,'',undefined,consoleId,1);assert.equal(store.register('owner',null,'',undefined,consoleId,2,undefined,undefined,{authClientId:'Web',policyMode:'dynamic'}).sessionKey,consoleOld.sessionKey);
 await store.revoke(old.session.hash);assert.throws(()=>store.register('owner','Desktop','fixture',key,id,2,1,undefined,dynamic),(e:any)=>e.code==='REGISTRATION_ATTEMPT_EXPIRED');
 const next=store.register('owner','Desktop','fixture',key,randomUUID(),3,2,undefined,dynamic);assert.ok(next.session.authClientId);assert.throws(()=>store.register('owner','Desktop','fixture',key,id,4,1,undefined,dynamic),(e:any)=>e.code==='SESSION_SUPERSEDED');store.close();
});
