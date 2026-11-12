import {RamOwner,OwnerFailure,type Binding,type TokenRequest} from '../../desktop/src/ram-owner.ts';
import {jwtChecks} from '../../desktop/src/native-jwt.ts';
import type {AccessTokenRequest} from '../../../../sdk-js/src/index.js';
import type {WebConfig,BusinessAuthorization} from './types.ts';
const scopes=['openid','offline_access','profile','email','connect'];
export function validateConfig(c:WebConfig):Binding {
 const issuer=new URL(c.issuer),server=new URL(c.server),resource=new URL(c.resource),redirect=new URL(c.redirectUri);
 const hasCredentials=(u:URL)=>Boolean(u.username||u['pass'+'word' as keyof URL]);
 if(issuer.protocol!=='https:'||issuer.pathname!=='/oidc'||issuer.href!==c.issuer||issuer.search||issuer.hash||hasCredentials(issuer)||server.protocol!=='https:'||server.origin!==c.server||resource.protocol!=='https:'||resource.href!==c.resource||resource.hash||hasCredentials(resource)||redirect.origin!==location.origin||redirect.pathname!=='/auth/callback'||redirect.search||redirect.hash||redirect.href!==c.redirectUri||!/^[-_A-Za-z0-9]{1,21}$/.test(c.clientId)||!/^[-_A-Za-z0-9]{1,21}$/.test(c.productId))throw new OwnerFailure('BINDING_MISMATCH');
 return {issuer:c.issuer,endpoint:issuer.origin,clientId:c.clientId,resource:c.resource,scopes,redirectUri:c.redirectUri};
}
/** Owns official PKCE, refresh rotation and all tokens in RAM; never returned as a view. */
export class SpaAuthorization {
 #owner:RamOwner; #ended=false; #generation:number; #binding:Binding;
 #popup:Window|null=null; #state:string|undefined; #finish:((url?:string)=>void)|undefined;
 #listener:((e:MessageEvent)=>void)|undefined; #timer:ReturnType<typeof setInterval>|undefined;
 constructor(config:WebConfig,generation:number){
  const binding=validateConfig(config);this.#binding=binding;this.#generation=generation;
  this.#owner=new RamOwner(generation,binding,{
   ...jwtChecks(binding),
   fetch:async(_operation,r)=>{
    const u=new URL(r.url),allowed=[binding.issuer+'/.well-known/openid-configuration',binding.issuer+'/jwks',binding.issuer+'/token'];
    if(!allowed.includes(u.href)||!['GET','POST'].includes(r.method))throw new OwnerFailure('BINDING_MISMATCH');
    const response=await fetch(u,{method:r.method,headers:r.headers,body:r.body,credentials:'omit',redirect:'error',cache:'no-store'});
    const reader=response.body?.getReader();const chunks:Uint8Array[]=[];let n=0;
    if(reader)try{for(;;){const {done,value}=await reader.read();if(done)break;n+=value.byteLength;if(n>131072){await reader.cancel();throw new OwnerFailure('OUTCOME_UNKNOWN');}chunks.push(value);}}finally{reader.releaseLock();}
    const bytes=new Uint8Array(n);let at=0;for(const b of chunks){bytes.set(b,at);at+=b.length;}
    return {status:response.status,headers:{'content-type':'application/json'},body:new TextDecoder('utf-8',{fatal:true}).decode(bytes)};
   },
   navigate:async raw=>{
    const u=new URL(raw);if(u.origin+u.pathname!==binding.issuer+'/auth'||u.searchParams.get('client_id')!==binding.clientId||u.searchParams.get('redirect_uri')!==binding.redirectUri||!u.searchParams.get('state')||!this.#popup||this.#popup.closed)throw new OwnerFailure('AUTH_UNAVAILABLE');
    this.#state=u.searchParams.get('state')!;this.#popup.location.href=u.href;
   },
  });
 }
 /** Call from the click stack: the window is created before any await. */
 login():Promise<void>{
  if(this.#ended||this.#popup)return Promise.reject(new OwnerFailure('BUSY'));
  this.#popup=window.open('about:blank','_blank','popup,width=520,height=720');
  if(!this.#popup)return Promise.reject(new OwnerFailure('AUTH_UNAVAILABLE'));
  const popup=this.#popup;const started=performance.now();
  const callback=new Promise<string>((resolve,reject)=>{
   this.#finish=url=>url?resolve(url):reject(new OwnerFailure('CANCELLED'));
   this.#listener=e=>{
    if(e.origin!==location.origin||e.source!==popup||!e.data||e.data.type!=='cybergrid-workspace-callback'||typeof e.data.url!=='string'||e.data.url.length>32768)return;
    try{const u=new URL(e.data.url);if(u.origin+u.pathname!==this.#binding.redirectUri||u.hash||u.searchParams.getAll('state').length!==1||u.searchParams.get('state')!==this.#state)return;this.#finish?.(u.href);}catch{/* Ignore unrelated messages. */}
   };window.addEventListener('message',this.#listener);
   this.#timer=setInterval(()=>{if(popup.closed||performance.now()-started>180000)this.#finish?.();},200);
  });
  // Attach rejection immediately while discovery/navigation is still owned.
  void callback.catch(()=>{});
  return (async()=>{try{await this.#owner.signIn();const url=await callback;if(this.#ended)throw new OwnerFailure('CANCELLED');await this.#owner.callback(url);await this.#owner.bindSdk(this.#generation);}
   finally{this.cleanupPopup();}})();
 }
 private cleanupPopup(){if(this.#listener)window.removeEventListener('message',this.#listener);clearInterval(this.#timer);this.#popup?.close();this.#popup=null;this.#listener=undefined;this.#finish=undefined;this.#state=undefined;}
 private request(forceRefresh:boolean,minValidity:number):TokenRequest{return {requestId:crypto.randomUUID(),ownerGeneration:this.#generation,sdkGeneration:this.#generation,issuer:this.#binding.issuer,clientId:this.#binding.clientId,resource:this.#binding.resource,scopes:this.#binding.scopes,minValidity,forceRefresh,reason:'connect'};}
 async coordination(r:AccessTokenRequest):Promise<string>{
  if(this.#ended||r.issuer!==this.#binding.issuer||r.clientId!==this.#binding.clientId||r.resource!==this.#binding.resource||r.scopes.length!==1||r.scopes[0]!=='connect'||r.signal.aborted)throw new OwnerFailure('BINDING_MISMATCH');
  const token=await this.#owner.accessToken(this.request(r.forceRefresh,r.minValidity));
  if(this.#ended||r.signal.aborted)throw new OwnerFailure('CANCELLED');return token;
 }
 business():BusinessAuthorization{return Object.freeze({fresh:async({signal}:{signal:AbortSignal})=>{
  if(this.#ended||signal.aborted)throw new OwnerFailure('CANCELLED');
  const token=await this.#owner.accessToken(this.request(true,60));
  if(this.#ended||signal.aborted)throw new OwnerFailure('CANCELLED');return token;
 }});}
 async end():Promise<void>{this.#ended=true;this.#finish?.();this.cleanupPopup();await this.#owner.end();}
}
/** Callback has no storage or third-party code; remove secrets before sending to opener. */
export function deliverCallback():boolean {
 if(location.pathname!=='/auth/callback')return false;
 const raw=location.href;history.replaceState(null,'',location.origin+'/auth/callback');
 if(window.opener&&!window.opener.closed){window.opener.postMessage({type:'cybergrid-workspace-callback',url:raw},location.origin);window.close();}
 document.body.textContent='授权回调已返回。若原窗口已关闭，请回到登录页重新授权。';return true;
}
