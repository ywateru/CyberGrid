// Real Node/Redis/ES384 + official RAM owner + JS SDK. Adapter lifecycle is a fixture,
// not Noise, public TLS, Cloudflare, cloud Logto or browser-device acceptance.
import type {TestContext} from 'node:test';import assert from 'node:assert/strict';import {createServer} from 'node:http';import {once} from 'node:events';import {createHash,randomBytes,randomUUID} from 'node:crypto';
import {generateKeyPair,exportJWK,createLocalJWKSet,SignJWT} from 'jose';
import {redisFixture} from '../../../../server/test/redis-fixture.ts';import {RedisStore,redisTransport} from '../../../../server/src/redis.ts';import {createService} from '../../../../server/src/service.ts';import {serveAPI} from '../../../../server/src/http.ts';import {coordinatorAuthConfiguration} from '../../../../server/src/deployment-config.ts';import {LogtoIdentity} from '../../../../server/src/identity.ts';import {ProductPolicy} from '../../../../server/src/products.ts';import {EndpointPolicy} from '../../../../server/src/endpoints.ts';
import {mountRemoteWorkspace} from '../src/index.ts';
export async function uiFixture(t:TestContext){
 const f=await redisFixture(t);assert(f);const store=new RedisStore(redisTransport({REDIS_URL:f.url}),f.prefix,{},'finite-v2');await store.ready();t.after(()=>store.close());
 const frontend='https://workspace.example.test',coord='https://coord.example.test';
 const config={server:coord,issuer:'https://identity.example.test/oidc',resource:'https://resource.example.test/api',clientId:'browser',productId:'workspace',redirectUri:frontend+'/auth/callback'};
 const auth=coordinatorAuthConfiguration({LOGTO_ISSUER:config.issuer,LOGTO_RESOURCE:config.resource,LOGTO_BROWSER_CLIENT_ID:config.clientId});
 const pair=await generateKeyPair('ES384'),jwk={...await exportJWK(pair.publicKey),kid:'integration',alg:'ES384'},identity=new LogtoIdentity(auth,['ES384'],createLocalJWKSet({keys:[jwk]}));
 const products=new ProductPolicy(JSON.stringify({version:1,products:[{id:'workspace',name:'Workspace',nativeClientIds:['workspace','native'],browserClientIds:['browser']}]}));
 const handler=createService({store,identity,directory:products,products,browserClientId:'browser',browserOrigins:new Set([frontend]),endpointPolicy:new EndpointPolicy(),authRateLimit:1000});
 const token=(client:string,audience=config.resource)=>new SignJWT(audience===config.resource?{client_id:client,scope:'connect'}:{}).setProtectedHeader({alg:'ES384',kid:jwk.kid}).setSubject('owner').setIssuer(config.issuer).setAudience(audience).setIssuedAt().setExpirationTime('1h').sign(pair.privateKey);
 let state='',challenge='',refresh='initial',rotations=0;
 const server=createServer((req,res)=>{
  if(req.url?.startsWith('/api/v1/')){void serveAPI(req,res,handler);return;}
  void(async()=>{res.setHeader('content-type','application/json');
   if(req.url==='/oidc/.well-known/openid-configuration'){res.end(JSON.stringify({issuer:config.issuer,authorization_endpoint:config.issuer+'/auth',token_endpoint:config.issuer+'/token',jwks_uri:config.issuer+'/jwks',response_types_supported:['code'],subject_types_supported:['public'],id_token_signing_alg_values_supported:['ES384']}));return;}
   if(req.url==='/oidc/jwks'){res.end(JSON.stringify({keys:[jwk]}));return;}
   if(req.url!=='/oidc/token'){res.writeHead(404).end('{}');return;}
   let body='';for await(const c of req)body+=c;const p=new URLSearchParams(body);
   const valid=p.get('grant_type')==='authorization_code'?createHash('sha256').update(p.get('code_verifier')??'').digest('base64url')===challenge:p.get('refresh_token')===refresh;
   if(!valid){res.writeHead(400).end('{}');return;}refresh='rotation-'+(++rotations);
   res.end(JSON.stringify({access_token:await token('browser'),refresh_token:refresh,id_token:await token('browser','browser'),scope:'connect',expires_in:3600,token_type:'Bearer'}));
  })().catch(()=>res.writeHead(500).end('{}'));
 });server.listen(0,'127.0.0.1');await once(server,'listening');const address=server.address();assert(address&&typeof address==='object');const local='http://127.0.0.1:'+address.port;
 t.after(async()=>{server.closeAllConnections();await new Promise<void>(r=>server.close(()=>r()));});
 let unavailable=false;const domListeners=new Map<string,()=>void>();
 const originalFetch=globalThis.fetch,descriptors=new Map(['window','location','document'].map(k=>[k,Object.getOwnPropertyDescriptor(globalThis,k)]));
 const listeners=new Set<(e:MessageEvent)=>void>();let navigation!:()=>void;const navigated=new Promise<void>(r=>navigation=r);
 const popup={closed:false,close(){this.closed=true;},location:{set href(raw:string){const u=new URL(raw);state=u.searchParams.get('state')!;challenge=u.searchParams.get('code_challenge')!;navigation();}}};
 Object.defineProperty(globalThis,'location',{value:{origin:frontend,pathname:'/',href:frontend+'/'},configurable:true});
 Object.defineProperty(globalThis,'window',{value:{open:()=>popup,confirm:()=>true,addEventListener:(k:string,fn:(e:MessageEvent)=>void)=>{if(k==='message')listeners.add(fn);else domListeners.set(k,fn as unknown as ()=>void);},removeEventListener:(_k:string,fn:(e:MessageEvent)=>void)=>listeners.delete(fn)},configurable:true});
 globalThis.fetch=async(input,init)=>{const u=new URL(String(input));if(unavailable&&u.origin===coord&&u.pathname.includes('/directory'))return new Response('{}',{status:503});assert([coord,'https://identity.example.test'].includes(u.origin));const headers=new Headers(init?.headers);if(u.origin===coord)headers.set('origin',frontend);return originalFetch(local+u.pathname+u.search,{...init,headers});};
 t.after(()=>{globalThis.fetch=originalFetch;for(const[k,d]of descriptors){if(d)Object.defineProperty(globalThis,k,d);else Reflect.deleteProperty(globalThis,k);}});

 const call=async(path:string,key:string|undefined,method='GET',body?:unknown)=>{const r=await originalFetch(local+'/api/v1'+path,{method,headers:{'content-type':'application/json','x-cybergrid-epoch':store.epoch,...(key?{authorization:'Bearer '+key}:{})},body:body===undefined?undefined:JSON.stringify(body)});assert.equal(r.status,200);return r.json() as Promise<any>;};
 const base={appId:'native',accessToken:await token('native'),registrationKey:randomBytes(32).toString('base64url')};
 const ch=await call('/auth/registration/challenge',undefined,'POST',base),admit=await call('/auth/registration/admit',undefined,'POST',{...base,challenge:ch.challenge,requestId:randomUUID()});
 const native=await call('/auth/exchange',undefined,'POST',{...base,admissionTicket:admit.admissionTicket,deviceName:'Native fixture',requestId:randomUUID(),registrationRevision:1,coordinationProtocol:'memory-v1',coordinatorEpoch:store.epoch});
 const descriptor={id:'workspace',protocol:'anywork.remote.v1',providerId:'quick-tunnel',url:'wss://fixture.trycloudflare.com/remote/v1/ws',installationId:'synthetic-installation',publicKey:randomBytes(32).toString('base64url'),addressRevision:1,expiresAt:new Date(Date.now()+180000).toISOString(),limits:{maxFrameBytes:65535,maxStreams:2,maxPendingRequests:8}};
 await call('/devices/self',native.sessionKey,'PUT',{ports:[],tunnelUrl:null,certificateFingerprint:'a'.repeat(64),bridges:[descriptor],incarnation:native.incarnation,publishRevision:1});

 class Element {
  textContent='';innerHTML='';children:Element[]=[];onclick:(()=>void)|undefined;disabled=false;
  append(...items:Element[]){this.children.push(...items);} replaceChildren(...items:Element[]){this.children=items;}
  querySelector(selector:string){return nodes.get(selector.slice(6,-1));}
 }
 const nodes=new Map(['login','logout','status','adapter','devices','cancel','info','list','create','stream','echo','output','error'].map(k=>[k,new Element()]));
 Object.defineProperty(globalThis,'document',{value:{createElement:()=>new Element(),visibilityState:'visible',addEventListener(k:string,f:()=>void){domListeners.set(k,f);},removeEventListener(k:string){domListeners.delete(k);}},configurable:true});

 const wait=async(fn:()=>boolean)=>{const until=performance.now()+6000;while(!fn()){assert(performance.now()<until,'timed out');await new Promise(r=>setTimeout(r,10));}};
 const mount=(dependencies:Parameters<typeof mountRemoteWorkspace>[2])=>mountRemoteWorkspace(new Element() as any,config,dependencies);
 const login=async()=>{nodes.get('login')!.onclick!();await navigated;for(const listener of listeners)listener({origin:frontend,source:popup,data:{type:'cybergrid-workspace-callback',url:config.redirectUri+'?code=synthetic-code&state='+state}} as MessageEvent);await wait(()=>nodes.get('devices')!.children.length===1);};
 const choose=()=>nodes.get('devices')!.children[0].children[1].onclick!();
 return {nodes,config,descriptor,identity,mount,login,choose,wait,foreground:()=>domListeners.get('visibilitychange')?.(),setUnavailable:(value:boolean)=>{unavailable=value;}};
}
