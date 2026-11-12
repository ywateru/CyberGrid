import {test, type TestContext} from 'node:test';
import assert from 'node:assert/strict';
import {readFile} from 'node:fs/promises';
import {createServer, type ServerResponse} from 'node:http';
import {webcrypto} from 'node:crypto';
import vm from 'node:vm';
import {generateKeyPair, exportJWK, SignJWT} from 'jose';

type Mode='normal'|'permission'|'json'|'shape'|'capacity'|'oversize'|'removed'|'hold';
const catalog={source:'static',apps:[{id:'Core',name:'Core Product',type:'Native',isDeviceFlow:false}]};
const snapshot=(name:string)=>({devices:[{id:'FixtureDevice',name,online:true,lastSeen:'2026-11-10T00:00:00.000Z',ports:[]}]});
async function until(check:()=>boolean){const deadline=performance.now()+3000;while(!check()){assert.ok(performance.now()<deadline,'fixture HTTP operation must complete');await new Promise(r=>setTimeout(r,5));}}
async function loggedIn(t:TestContext){
 const elements=new Map<string,any>(),storage=new Map<string,string>();let assigned='',catalogMode:Mode='normal',holdDevice=false,deviceName='InitialDevice',timer:()=>Promise<void>=async()=>{};
 let heldDevice:ServerResponse|undefined,heldCatalog:ServerResponse|undefined;const calls:string[]=[];
 const pair=await generateKeyPair('ES384'),jwk={...await exportJWK(pair.publicKey),kid:'FixtureKey',alg:'ES384'};
 const config={issuer:'https://fixture.invalid/oidc',clientId:'FixtureSPA',resource:'https://fixture.invalid/api',scope:'openid connect',authorizationEndpoint:'https://fixture.invalid/oidc/auth',tokenEndpoint:'https://fixture.invalid/oidc/token',revocationEndpoint:'https://fixture.invalid/oidc/revoke',jwksUri:'https://fixture.invalid/oidc/jwks',redirectUri:'',coordination:{protocol:'memory-v1',epoch:'FixtureEpoch'}};
 const send=(res:ServerResponse,status:number,data:unknown)=>{res.writeHead(status,{'content-type':'application/json','x-cybergrid-epoch':'FixtureEpoch'});res.end(JSON.stringify(data));};
 const server=createServer(async(req,res)=>{
  calls.push(req.url!);
  if(req.url==='/api/v1/console/auth/config')return send(res,200,config);
  if(req.url==='/oidc/token'){
   const id=await new SignJWT({nonce:new URL(assigned).searchParams.get('nonce')}).setSubject('FixtureUser').setIssuer(config.issuer).setAudience(config.clientId).setIssuedAt().setExpirationTime('10m').setProtectedHeader({alg:'ES384',kid:'FixtureKey'}).sign(pair.privateKey);
   return send(res,200,{id_token:id,access_token:'Fixture.Resource.Token',expires_in:600});
  }
  if(req.url==='/oidc/jwks')return send(res,200,{keys:[jwk]});
  if(req.url==='/api/v1/console/auth/exchange')return send(res,200,{purpose:'console',user:{id:'FixtureUser'},sessionKey:'FixtureSession',coordinatorEpoch:'FixtureEpoch'});
  if(req.url==='/api/v1/apps'){
   if(catalogMode==='hold'){heldCatalog=res;return;}
   if(catalogMode==='permission')return send(res,403,{error:{code:'FORBIDDEN',message:'untrusted-provider-detail'}});
   if(catalogMode==='capacity')return send(res,413,{error:{code:'DIRECTORY_TOO_LARGE',message:'untrusted-provider-detail'}});
   if(catalogMode==='json'){res.end('{untrusted-provider-detail');return;}
   if(catalogMode==='shape')return send(res,200,{source:'unexpected',apps:[]});
   if(catalogMode==='oversize'){res.end(' '.repeat(2*1024*1024+1));return;}
   return send(res,200,catalogMode==='removed'?{source:'static',apps:[]}:catalog);
  }
  if(req.url?.startsWith('/api/v1/console/devices')){if(holdDevice){heldDevice=res;return;}return send(res,200,snapshot(deviceName));}
  return send(res,404,{});
 });
 await new Promise<void>(resolve=>server.listen(0,'127.0.0.1',resolve));
 const address=server.address();assert.ok(address&&typeof address==='object');const origin='http://127.0.0.1:'+address.port;config.redirectUri=origin+'/console/callback/';
 t.after(async()=>{server.closeAllConnections();await new Promise<void>(resolve=>server.close(()=>resolve()));});
 const element=()=>({disabled:false,hidden:false,value:'',textContent:'',children:[] as any[],listeners:{} as Record<string,Function>,classList:{toggle(){}},addEventListener(name:string,fn:Function){this.listeners[name]=fn;},append(...children:any[]){this.children.push(...children);},replaceChildren(...children:any[]){this.children=children;}});
 const context=vm.createContext({document:{getElementById(id:string){if(!elements.has(id))elements.set(id,element());return elements.get(id);},createElement:element},window:{addEventListener(){}},location:{search:'',pathname:'/console/',origin,assign:(url:string)=>assigned=String(url)},history:{replaceState(){}},sessionStorage:{getItem:(key:string)=>storage.get(key),setItem:(key:string,value:string)=>storage.set(key,value),removeItem:(key:string)=>storage.delete(key)},setInterval:(fn:()=>Promise<void>)=>{timer=fn;return 1;},clearInterval(){},AbortSignal,AbortController,URLSearchParams,URL,crypto:webcrypto,TextEncoder,TextDecoder,Uint8Array,btoa,atob,Date,fetch:async(input:string,options:RequestInit)=>{
  const url=new URL(String(input),origin);
  // Real HTTP deliberately ignores abort: only script fencing may reject stale results.
  return fetch(origin+url.pathname+url.search,{...options,signal:undefined});
 }});
 const source=await readFile(new URL('../public/console/console.js',import.meta.url),'utf8');vm.runInContext(source,context);const run=(code:string)=>vm.runInContext(code,context);
 await until(()=>!run('busy'));await run('beginLogin()');assert.ok(assigned);
 await run(`callback(new URLSearchParams({state:${JSON.stringify(new URL(assigned).searchParams.get('state'))},code:'FixtureCode'}))`);
 assert.match(elements.get('device-subtitle').textContent,/1 台在线/);
 return {run,calls,subtitle:()=>elements.get('device-subtitle').textContent,name:()=>elements.get('devices').children[0]?.children[0]?.children[0]?.textContent,message:()=>elements.get('message').textContent,refresh:()=>elements.get('refresh').listeners.click(),tick:()=>timer(),mode:(mode:Mode)=>catalogMode=mode,fresh:(name:string)=>{holdDevice=false;deviceName=name;},holdOld:async()=>{holdDevice=true;const pending=timer();await until(()=>!!heldDevice);return {pending};},releaseOld:()=>{assert.ok(heldDevice);send(heldDevice,200,snapshot('StaleDevice'));heldDevice=undefined;},waitCatalog:()=>until(()=>!!heldCatalog),releaseCatalog:()=>{assert.ok(heldCatalog);send(heldCatalog,200,catalog);heldCatalog=undefined;}};
}
for(const mode of ['permission','json','shape','capacity','oversize'] as const)test('real login: '+mode+' catalog failure fences stale devices; refresh button can recover',async t=>{
 const f=await loggedIn(t),old=await f.holdOld();f.mode(mode);await f.refresh();assert.equal(f.subtitle(),'状态未知');
 f.releaseOld();await old.pending;assert.equal(f.subtitle(),'状态未知');assert.doesNotMatch(f.message(),/untrusted-provider-detail/);
 const count=f.calls.length;await f.tick();assert.equal(f.calls.length,count,'timer must not bypass invalid catalog');
 f.mode('normal');f.fresh('RecoveredDevice');await f.refresh();assert.match(f.subtitle(),/1 台在线/);assert.equal(f.name(),'RecoveredDevice','real UI button must revalidate catalog then read fresh devices');
});
test('real login: starting a newer catalog fences devices before its response, including unchanged product',async t=>{
 const f=await loggedIn(t),old=await f.holdOld();f.mode('hold');const refreshed=f.refresh();await f.waitCatalog();
 f.releaseOld();await old.pending;assert.equal(f.subtitle(),'状态未知','catalog has not yet authorized any fresh device result');
 const count=f.calls.length;await f.tick();assert.equal(f.calls.length,count);
 f.fresh('NewCatalogDevice');f.releaseCatalog();await refreshed;assert.equal(f.name(),'NewCatalogDevice');
});
test('real login: successful same-product catalog cannot authorize earlier device success',async t=>{
 const f=await loggedIn(t),old=await f.holdOld();f.fresh('FreshDevice');await f.refresh();assert.equal(f.name(),'FreshDevice');
 f.releaseOld();await old.pending;assert.equal(f.name(),'FreshDevice');
});
test('real login: product removal fences old reads and timer/selection, with actual UI recovery',async t=>{
 const f=await loggedIn(t),old=await f.holdOld();f.mode('removed');await f.refresh();f.releaseOld();await old.pending;assert.equal(f.subtitle(),'状态未知');
 const count=f.calls.length;f.run("$('app-select').value='Core'");await f.run("$('app-select').listeners.change()");await f.tick();assert.equal(f.calls.length,count,'removed product cannot become authorized through selection or timer');
 f.mode('normal');f.fresh('AllowedAgain');await f.refresh();assert.equal(f.name(),'AllowedAgain');
});
