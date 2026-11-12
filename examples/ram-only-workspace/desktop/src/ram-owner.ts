import LogtoClient, {type ClientAdapter, type JwtVerifier} from '@logto/client';
import {generateState,generateCodeVerifier,generateCodeChallenge} from '@logto/browser';
export type Binding={issuer:string;endpoint:string;clientId:string;resource:string;scopes:string[];redirectUri:string};
export type TokenRequest={requestId:string;ownerGeneration:number;sdkGeneration:number;issuer:string;clientId:string;resource:string;scopes:string[];minValidity:number;forceRefresh:boolean;reason:'connect'|'coordination-recovery'|'authorization-rejected'};
export type Failure='AUTH_UNAVAILABLE'|'OUTCOME_UNKNOWN'|'BINDING_MISMATCH'|'BUSY'|'CANCELLED';
export class OwnerFailure extends Error {readonly code:Failure;constructor(code:Failure){super(code);this.code=code;}}
export type NativeRequest={url:string;method:string;headers:Record<string,string>;body?:string};
export type NativeResponse={status:number;body:string;headers:Record<string,string>};
export type Dependencies={fetch:(operationId:number,request:NativeRequest)=>Promise<NativeResponse>;navigate:(url:string)=>Promise<void>;verify:(getConfig:()=>Promise<{issuer:string;jwksUri:string}>,transport:typeof fetch)=>JwtVerifier;validateAccess:(token:string,binding:Binding)=>Promise<{subject:string;expiresAt:number;scopes:string[]}>};
// Only this class calls the official client's grant-consuming APIs. No Web Storage.
export class RamOwner {
 #memory=new Map<string,string>();
 #received:NativeResponse[]=[]; // newest replies retained before official parsing/verification
 #queue:Promise<void>=Promise.resolve();#queued=0;#operation=0;
 #blocked:Failure|undefined;#ending=false;#ended=false;#subject:string|undefined;
 #sdkGeneration:number|undefined;
 #client:LogtoClient;
 readonly generation:number;readonly binding:Binding;#deps:Dependencies;
 constructor(generation:number,binding:Binding,deps:Dependencies){
  this.generation=generation;this.binding=structuredClone(binding);Object.freeze(this.binding.scopes);Object.freeze(this.binding);binding=this.binding;this.#deps=deps;
  const transport:typeof fetch=async(input,init)=>{
   if(this.#blocked||this.#ended)throw new OwnerFailure(this.#blocked??'AUTH_UNAVAILABLE');
   const r=new Request(input,init);const headers:Record<string,string>={};r.headers.forEach((v,k)=>headers[k]=v);
   const body=['GET','HEAD'].includes(r.method)?undefined:await r.text();let result:NativeResponse;
   try{result=await deps.fetch(++this.#operation,{url:r.url,method:r.method,headers,body});}
   catch{if(r.method==='POST')this.#blocked='OUTCOME_UNKNOWN';throw new OwnerFailure(this.#blocked??'AUTH_UNAVAILABLE');}
   if(r.method==='POST'){
    // Keep the actual bounded reply even if official callback ID verification fails.
    this.#received.push(result);if(this.#received.length>2)this.#received.shift();
    if(result.status>=200&&result.status<300){
     try{const value=JSON.parse(result.body);if(!value||typeof value!=='object')throw Error();}
     catch{this.#blocked='OUTCOME_UNKNOWN';throw new OwnerFailure('OUTCOME_UNKNOWN');}
    }
   }
   // The official requester logs non-2xx bodies. Reject before it sees them.
   if(result.status<200||result.status>=300)throw new OwnerFailure('AUTH_UNAVAILABLE');
   return new Response([204,205,304].includes(result.status)?null:result.body,{status:result.status,headers:result.headers});
  };
  const adapter:ClientAdapter={storage:{getItem:async k=>this.#memory.get(k)??null,setItem:async(k,v)=>{this.#memory.set(k,v);},removeItem:async k=>{this.#memory.delete(k);}},fetch:transport,navigate:async(url,p)=>{if(p.for!=='sign-in')throw new OwnerFailure('AUTH_UNAVAILABLE');await deps.navigate(url);},generateState,generateCodeVerifier,generateCodeChallenge};
  // No requestTimeoutMs: the native owned transport controls timeout/unknown outcomes.
  this.#client=new LogtoClient({endpoint:binding.endpoint,appId:binding.clientId,resources:[binding.resource],scopes:binding.scopes},adapter,c=>deps.verify(()=>c.getOidcConfig(),transport));
 }
 private enqueue<T>(f:()=>Promise<T>):Promise<T>{
  if(this.#ending||this.#ended)return Promise.reject(new OwnerFailure('AUTH_UNAVAILABLE'));
  if(this.#queued>=2)return Promise.reject(new OwnerFailure('BUSY'));
  this.#queued++;
  const work=this.#queue.then(async()=>{if(this.#blocked)throw new OwnerFailure(this.#blocked);try{return await f();}catch(e){
   // A returned grant followed by SDK/verifier failure must never be silently reused.
   if(this.#received.length)this.#blocked??='AUTH_UNAVAILABLE';throw e instanceof OwnerFailure?e:new OwnerFailure('AUTH_UNAVAILABLE');
  }}).finally(()=>{this.#queued--;});
  this.#queue=work.then(()=>{},()=>{});return work;
 }
 signIn():Promise<void>{return this.enqueue(()=>this.#client.signIn({redirectUri:this.binding.redirectUri}));}
 callback(url:string):Promise<void>{return this.enqueue(()=>this.#client.handleSignInCallback(url));}
 async bindSdk(generation:number):Promise<void>{await this.settled();if(this.#ending||this.#ended||this.#blocked)throw new OwnerFailure(this.#blocked??'AUTH_UNAVAILABLE');if(!Number.isSafeInteger(generation)||generation<1)throw new OwnerFailure('BINDING_MISMATCH');this.#sdkGeneration=generation;}
 accessToken(r:TokenRequest):Promise<string>{
  const b=this.binding;
  if(r.ownerGeneration!==this.generation||r.sdkGeneration!==this.#sdkGeneration||r.issuer!==b.issuer||r.clientId!==b.clientId||r.resource!==b.resource||[...r.scopes].sort().join(' ')!==[...b.scopes].sort().join(' ')||!Number.isInteger(r.minValidity)||r.minValidity<1||r.minValidity>3600)return Promise.reject(new OwnerFailure('BINDING_MISMATCH'));
  return this.enqueue(async()=>{
   if(r.sdkGeneration!==this.#sdkGeneration)throw new OwnerFailure('CANCELLED');
   if(r.forceRefresh)await this.#client.clearAccessToken();
   let token=await this.#client.getAccessToken(b.resource);let claims=await this.#deps.validateAccess(token,b);
   if(claims.expiresAt-Date.now()/1000<r.minValidity&&!r.forceRefresh){await this.#client.clearAccessToken();token=await this.#client.getAccessToken(b.resource);claims=await this.#deps.validateAccess(token,b);}
   if(claims.expiresAt-Date.now()/1000<r.minValidity||!b.scopes.filter(s=>!['openid','profile','email','offline_access'].includes(s)).every(s=>claims.scopes.includes(s))||(this.#subject!==undefined&&claims.subject!==this.#subject))throw new OwnerFailure('AUTH_UNAVAILABLE');
   this.#subject=claims.subject;return token;
  });
 }
 async settled():Promise<void>{await this.#queue;}
 async end():Promise<void>{this.#ending=true;await this.settled();await this.#client.clearAllTokens();this.#memory.clear();this.#received=[];this.#subject=undefined;this.#ended=true;}
 diagnostics(){return {generation:this.generation,pending:this.#queued,escrowReplies:this.#received.length,blocked:this.#blocked??null,ended:this.#ended};}
}
