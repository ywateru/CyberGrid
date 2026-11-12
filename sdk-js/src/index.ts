import {connectControlledBridge,type TrustedBridgeAdapter,type SecureBridge,type BridgeConnectOptions} from './bridge.js';
export type {TrustedBridgeAdapter,AuthenticatedBridgePeer,BridgeTarget,SecureBridge,SecureBridgeStream,BridgeState,BridgeConnectOptions,AdapterByteStream} from './bridge.js';
import {type EndpointPolicy,policyConfiguration} from './endpoints.js';
import {type DirectorySnapshot,type DirectoryEvent,type DirectoryWatch,type DirectoryWatchOptions,parseDevices,parseDirectory} from './directory.js';
export type {EndpointProvider,EndpointPolicy,BrowserBridge,BridgeLimits} from './endpoints.js';
export type {DirectorySnapshot,DirectoryDevice,DirectoryEvent,DirectoryWatch,DirectoryWatchOptions} from './directory.js';
export type LoginState='disconnected'|'connecting'|'connected'|'recovering';
export interface RequestOptions {signal?:AbortSignal}
export type TokenRequestReason='connect'|'coordination-recovery'|'authorization-rejected';
export interface AccessTokenRequest {issuer:string;clientId:string;resource:string;scopes:readonly string[];minValidity:number;forceRefresh:boolean;requestReason:TokenRequestReason;generation:number;signal:AbortSignal}
export type ErrorStage='idle'|'configuration'|'host-authorization'|'exchange'|'directory'|'disconnect'|'bridge';
export type StateReason='idle'|'connect-requested'|'coordination-recovery'|'configuration-request'|'configuration-retry'|'host-token-request'|'authorization-rejected'|'exchange-request'|'exchange-outcome-unknown'|'connected'|'directory-request'|'directory-ready'|'disconnected'|'host-business-authorization'|'invalid-configuration'|'configuration-mismatch'|'login-required'|'authentication-failed'|'session-unavailable'|'network-error'|'timeout'|'cancelled'|'invalid-response'|'forbidden'|'invalid-input'|'stale-operation'|'not-found'|'identity-changed'|'directory-too-large'|'unsupported-bridge'|'bridge-unavailable'|'bridge-closed'|'bridge-identity-mismatch'|'bridge-capacity-exceeded'|'bridge-protocol-error';
export type HostBusinessAuthorization='unknown'|'authorized'|'denied';
export interface ClientSnapshot {
 readonly revision:number;readonly generation:number;readonly epoch:string|null;readonly loginState:LoginState;
 readonly identity:'unknown'|'requesting'|'verified'|'login-required'|'changed';readonly coordination:LoginState|'unavailable';
 readonly publication:'unknown';readonly reachability:'unknown';readonly hostBusinessAuthorization:HostBusinessAuthorization;
 readonly stage:ErrorStage;readonly reason:StateReason;readonly retryable:boolean;readonly retryAfterMs:number|null;readonly nextRetryAt:number|null;readonly diagnosticId:string|null;
}
export interface ClientOptions {
 baseUrl:string;issuer:string;clientId:string;resource:string;
 getAccessToken:(request:AccessTokenRequest)=>Promise<string|undefined>;
 timeoutMs?:number;minValidity?:number;
}
export interface Application {id:string;name:string;type:'Native';isDeviceFlow:false}
export interface Port {id:string;port:number;type:string;transport:'tcp';tunnelUrl:string|null}
export interface Device {id:string;name:string;online:boolean;lastSeen:string;ports:Port[]}
export type ErrorCode='INVALID_CONFIGURATION'|'CONFIGURATION_MISMATCH'|'LOGIN_REQUIRED'|'AUTHENTICATION_FAILED'|'SESSION_UNAVAILABLE'|'NETWORK_ERROR'|'TIMEOUT'|'CANCELLED'|'INVALID_RESPONSE'|'FORBIDDEN'|'INVALID_INPUT'|'STALE_OPERATION'|'NOT_FOUND'|'IDENTITY_CHANGED'|'DIRECTORY_TOO_LARGE'|'UNSUPPORTED_BRIDGE'|'BRIDGE_UNAVAILABLE'|'BRIDGE_CLOSED'|'BRIDGE_IDENTITY_MISMATCH'|'BRIDGE_CAPACITY_EXCEEDED'|'BRIDGE_PROTOCOL_ERROR';
const messages:Record<ErrorCode,string>={UNSUPPORTED_BRIDGE:'No trusted adapter supports this bridge',BRIDGE_UNAVAILABLE:'The selected bridge is unavailable',BRIDGE_CLOSED:'The business channel is closed',BRIDGE_IDENTITY_MISMATCH:'The authenticated bridge identity does not match',BRIDGE_CAPACITY_EXCEEDED:'The business channel operation limit is reached',BRIDGE_PROTOCOL_ERROR:'The business transport violated its declared limits',INVALID_CONFIGURATION:'Invalid CyberGrid configuration',CONFIGURATION_MISMATCH:'Trusted login configuration does not match',LOGIN_REQUIRED:'Connect using the host Logto login first',AUTHENTICATION_FAILED:'Host login authorization is unavailable',SESSION_UNAVAILABLE:'Console session is temporarily unavailable',NETWORK_ERROR:'CyberGrid request failed',TIMEOUT:'CyberGrid operation timed out',CANCELLED:'CyberGrid operation was cancelled',INVALID_RESPONSE:'CyberGrid returned an invalid response',FORBIDDEN:'Console operation is not permitted',INVALID_INPUT:'Invalid console query',STALE_OPERATION:'An outdated console response was discarded',NOT_FOUND:'The selected device or port was not found',IDENTITY_CHANGED:'Host login identity changed; disconnect before connecting again',DIRECTORY_TOO_LARGE:'The device directory exceeds the supported response size'};
const errorReasons:Record<ErrorCode,StateReason>={UNSUPPORTED_BRIDGE:'unsupported-bridge',BRIDGE_UNAVAILABLE:'bridge-unavailable',BRIDGE_CLOSED:'bridge-closed',BRIDGE_IDENTITY_MISMATCH:'bridge-identity-mismatch',BRIDGE_CAPACITY_EXCEEDED:'bridge-capacity-exceeded',BRIDGE_PROTOCOL_ERROR:'bridge-protocol-error',INVALID_CONFIGURATION:'invalid-configuration',CONFIGURATION_MISMATCH:'configuration-mismatch',LOGIN_REQUIRED:'login-required',AUTHENTICATION_FAILED:'authentication-failed',SESSION_UNAVAILABLE:'session-unavailable',NETWORK_ERROR:'network-error',TIMEOUT:'timeout',CANCELLED:'cancelled',INVALID_RESPONSE:'invalid-response',FORBIDDEN:'forbidden',INVALID_INPUT:'invalid-input',STALE_OPERATION:'stale-operation',NOT_FOUND:'not-found',IDENTITY_CHANGED:'identity-changed',DIRECTORY_TOO_LARGE:'directory-too-large'};
const stages:readonly ErrorStage[]=['idle','configuration','host-authorization','exchange','directory','disconnect','bridge'];
const reasons:ReadonlySet<string>=new Set([...Object.values(errorReasons),'idle','connect-requested','coordination-recovery','configuration-request','configuration-retry','host-token-request','authorization-rejected','exchange-request','exchange-outcome-unknown','connected','directory-request','directory-ready','disconnected','host-business-authorization']);
interface ErrorContext {stage?:ErrorStage;reason?:StateReason;retryable?:boolean;retryAfterMs?:number}
export class CyberGridError extends Error {
 readonly name='CyberGridError';readonly stage:ErrorStage;readonly reason:StateReason;readonly retryable:boolean;readonly retryAfterMs:number|null;readonly diagnosticId:string;
 constructor(readonly code:ErrorCode,readonly status?:number,context:ErrorContext={}){
  super(messages[Object.hasOwn(messages,code)?code:'INVALID_RESPONSE']);this.code=Object.hasOwn(messages,code)?code:'INVALID_RESPONSE';
  this.status=typeof status==='number'&&Number.isInteger(status)&&status>=100&&status<=599?status:undefined;
  this.stage=stages.includes(context.stage!)?context.stage!:'idle';this.reason=reasons.has(context.reason!)?context.reason!:errorReasons[this.code];
  this.retryable=typeof context.retryable==='boolean'?context.retryable:['NETWORK_ERROR','TIMEOUT','SESSION_UNAVAILABLE'].includes(this.code);
  this.retryAfterMs=typeof context.retryAfterMs==='number'&&Number.isSafeInteger(context.retryAfterMs)&&context.retryAfterMs>=0&&context.retryAfterMs<=30000?context.retryAfterMs:null;this.diagnosticId=requestId();
 }
}
const fail=(code:ErrorCode,status?:number,context?:ErrorContext):never=>{throw new CyberGridError(code,status,context);};
const contextual=(error:unknown,stage:ErrorStage):CyberGridError=>error instanceof CyberGridError?new CyberGridError(error.code,error.status,{stage,reason:error.reason,retryable:error.retryable,...(error.retryAfterMs===null?{}:{retryAfterMs:error.retryAfterMs})}):new CyberGridError('NETWORK_ERROR',undefined,{stage});
const KNOWN_SCOPES=Object.freeze(['openid','offline_access','profile','email','connect']);
const RESOURCE_SCOPES=Object.freeze(['connect']);
const MAX_RESPONSE=2*1024*1024;
const uuid=(v:unknown):v is string=>typeof v==='string'&&/^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/.test(v);
function object(value:unknown):Record<string,unknown>{if(!value||typeof value!=='object'||Array.isArray(value))return fail('INVALID_RESPONSE');return value as Record<string,unknown>;}
function text(value:unknown,max:number):string {if(typeof value!=='string'||!value.length||value.length>max||/[\u0000-\u001f\u007f]/.test(value))return fail('INVALID_RESPONSE');return value;}
function query(value:string,max:number):string {if(typeof value!=='string'||!value.length||value.length>max||/[\u0000-\u001f\u007f]/.test(value))return fail('INVALID_INPUT');return value;}
function httpsURL(value:string):URL {const u=new URL(value);if(u.protocol!=='https:'||u.username||u.password||u.search||u.hash)return fail('INVALID_CONFIGURATION');return u;}
function serviceOrigin(value:string):string {
 const u=new URL(value),loopback=u.hostname==='localhost'||u.hostname==='[::1]'||/^127(?:\.\d{1,3}){3}$/.test(u.hostname);
 if((u.protocol!=='https:'&&!(u.protocol==='http:'&&loopback))||u.username||u.password||u.pathname!=='/'||u.search||u.hash||value!==u.origin&&value!==u.origin+'/')return fail('INVALID_CONFIGURATION');
 return u.origin;
}
function portURL(value:unknown):string|null {
 if(value===undefined||value===null)return null;
 try {const u=new URL(text(value,512));if(u.protocol!=='https:'||!/^([a-z0-9]+-)*[a-z0-9]+\.trycloudflare\.com$/.test(u.hostname)||u.username||u.password||u.port||u.pathname!=='/'||u.search||u.hash)return fail('INVALID_RESPONSE');return u.origin;}catch{return fail('INVALID_RESPONSE');}
}
function requestId():string {
 const bytes=globalThis.crypto.getRandomValues(new Uint8Array(16));bytes[6]=(bytes[6]!&15)|64;bytes[8]=(bytes[8]!&63)|128;
 const h=Array.from(bytes,v=>v.toString(16).padStart(2,'0')).join('');return `${h.slice(0,8)}-${h.slice(8,12)}-${h.slice(12,16)}-${h.slice(16,20)}-${h.slice(20)}`;
}
/** Manual signal composition also works in browsers without AbortSignal.any/timeout. */
async function bounded<T>(work:(signal:AbortSignal)=>Promise<T>,parents:readonly (AbortSignal|undefined)[],timeout:number):Promise<T> {
 const controller=new AbortController();let timer:ReturnType<typeof setTimeout>|undefined;const removers:(()=>void)[]=[];
 const abort=(code:ErrorCode)=>{if(!controller.signal.aborted)controller.abort(new CyberGridError(code));};
 for(const parent of parents){if(!parent)continue;const listener=()=>abort('CANCELLED');parent.addEventListener('abort',listener,{once:true});removers.push(()=>parent.removeEventListener('abort',listener));if(parent.aborted)abort('CANCELLED');}
 if(controller.signal.aborted){for(const remove of removers)remove();return fail('CANCELLED');}
 timer=setTimeout(()=>abort('TIMEOUT'),timeout);
 let stop!:()=>void;
 const cancelled=new Promise<never>((_,reject)=>{stop=()=>reject(controller.signal.reason instanceof CyberGridError?controller.signal.reason:new CyberGridError('CANCELLED'));controller.signal.addEventListener('abort',stop,{once:true});});
 try {return await Promise.race([Promise.resolve().then(()=>{if(controller.signal.aborted)throw controller.signal.reason;return work(controller.signal);}),cancelled]);}
 finally {if(timer)clearTimeout(timer);controller.signal.removeEventListener('abort',stop);for(const remove of removers)remove();}
}
async function readJSON(response:Response,signal:AbortSignal):Promise<unknown> {
 const reader=response.body?.getReader();if(!reader)return fail('INVALID_RESPONSE');
 const cancel=()=>{void reader.cancel().catch(()=>{});};signal.addEventListener('abort',cancel,{once:true});
 const parts:Uint8Array[]=[];let bytes=0;
 try {for(;;){if(signal.aborted)throw signal.reason;const item=await reader.read();if(item.done)break;bytes+=item.value.byteLength;if(bytes>MAX_RESPONSE){cancel();return fail('INVALID_RESPONSE');}parts.push(item.value);}const body=new Uint8Array(bytes);let offset=0;for(const part of parts){body.set(part,offset);offset+=part.byteLength;}try{return JSON.parse(new TextDecoder('utf-8',{fatal:true}).decode(body));}catch{return fail('INVALID_RESPONSE');}}
 finally {signal.removeEventListener('abort',cancel);reader.releaseLock();}
}
interface HTTPResult {status:number;epoch:string|null;data:unknown;apiCode?:string;retryAfterMs?:number}
interface Session {key:string;epoch:string;expires:number;subject:string}
interface Pending {generation:number;epoch:string;requestId:string;known?:Session;token?:string;unknown?:boolean}
interface Attempt {pending:Pending}
interface Flight {generation:number;controller:AbortController;promise:Promise<void>;waiters:number;settled:boolean;reason:TokenRequestReason;stage:ErrorStage}

export class CyberGridClient {
 readonly #origin:string;readonly #issuer:string;readonly #clientId:string;readonly #resource:string;readonly #tokenProvider:ClientOptions['getAccessToken'];readonly #timeout:number;readonly #minValidity:number;
 #snapshot:ClientSnapshot=Object.freeze({revision:0,generation:0,epoch:null,loginState:'disconnected',identity:'unknown',coordination:'disconnected',publication:'unknown',reachability:'unknown',hostBusinessAuthorization:'unknown',stage:'idle',reason:'idle',retryable:false,retryAfterMs:null,nextRetryAt:null,diagnosticId:null});
 #listeners=new Map<(snapshot:ClientSnapshot)=>void,{revision:number}>();#notifications:ClientSnapshot[]=[];#notifying=false;
 #policy?:EndpointPolicy;#products?:ReadonlySet<string>;#hasPolicy=false;#policyReload?:Promise<void>;#policyUntrusted=false;#sampleStarts=new WeakMap<DirectorySnapshot,{start:number;sequence:number}>();#bridgeSlots=0;#watchSlots=0;#directoryRevision=0;#directorySequence=0;#bridgeInvalidators=new Set<()=>void>();
 #subject?:string;#state:LoginState='disconnected';#generation=0;#life=new AbortController();#session?:Session;#pending?:Pending;#flight?:Flight;#cleanups=new Map<string,Promise<void>>();#uncertain=new Set<Attempt>();#retired=new Map<string,Session>();
 constructor(options:ClientOptions){
  try {
   this.#origin=serviceOrigin(options.baseUrl);const issuer=httpsURL(options.issuer);if(issuer.pathname!=='/oidc'||issuer.href!==options.issuer)throw new CyberGridError('INVALID_CONFIGURATION');
   const resource=httpsURL(options.resource);if(resource.href!==options.resource)throw new CyberGridError('INVALID_CONFIGURATION');
   if(typeof options.clientId!=='string'||!/^[A-Za-z0-9_-]{1,21}$/.test(options.clientId)||typeof options.getAccessToken!=='function')throw new CyberGridError('INVALID_CONFIGURATION');
   this.#timeout=options.timeoutMs??15000;if(!Number.isSafeInteger(this.#timeout)||this.#timeout<100||this.#timeout>30000)throw new CyberGridError('INVALID_CONFIGURATION');
   this.#minValidity=options.minValidity??60;if(!Number.isSafeInteger(this.#minValidity)||this.#minValidity<1||this.#minValidity>3600)throw new CyberGridError('INVALID_CONFIGURATION');
   this.#issuer=options.issuer;this.#clientId=options.clientId;this.#resource=options.resource;this.#tokenProvider=options.getAccessToken;
  }catch{throw new CyberGridError('INVALID_CONFIGURATION');}
 }
 snapshot():ClientSnapshot{return this.#snapshot;}
 subscribe(callback:(snapshot:ClientSnapshot)=>void):()=>void {
  if(typeof callback!=='function')return fail('INVALID_INPUT');const entry={revision:this.#snapshot.revision};this.#listeners.set(callback,entry);
  try{callback(this.#snapshot);}catch{/* Host observers cannot interrupt the state machine. */}
  return ()=>{if(this.#listeners.get(callback)===entry)this.#listeners.delete(callback);};
 }
 setHostBusinessAuthorization(value:HostBusinessAuthorization):void {
  if(!['unknown','authorized','denied'].includes(value))return fail('INVALID_INPUT');if(value!==this.#snapshot.hostBusinessAuthorization)this.update({hostBusinessAuthorization:value});
 }
 private update(patch:Partial<Omit<ClientSnapshot,'revision'|'generation'|'loginState'|'publication'|'reachability'>>):void {
  if(patch.hostBusinessAuthorization==='denied'||patch.coordination&&patch.coordination!=='connected')for(const close of [...this.#bridgeInvalidators])close();
  this.#snapshot=Object.freeze({...this.#snapshot,...patch,revision:this.#snapshot.revision+1,generation:this.#generation,loginState:this.#state,publication:'unknown',reachability:'unknown'});this.#notifications.push(this.#snapshot);
  if(this.#notifying)return;this.#notifying=true;
  try{while(this.#notifications.length){const snapshot=this.#notifications.shift()!;for(const [callback,entry] of this.#listeners){if(entry.revision>=snapshot.revision)continue;entry.revision=snapshot.revision;try{callback(snapshot);}catch{/* Fixed, secret-free observer boundary. */}}}}finally{this.#notifying=false;}
 }
 private stage(f:Flight,stage:ErrorStage,reason:StateReason,patch:Partial<ClientSnapshot>={}):void {this.current(f);f.stage=stage;this.update({stage,reason,retryable:false,retryAfterMs:null,nextRetryAt:null,diagnosticId:null,...patch});}
 getLoginState():LoginState{return this.#state;}
 async connect(options:RequestOptions={}):Promise<void>{if(options.signal?.aborted)return fail('CANCELLED');if(this.#session&&this.#session.expires>Date.now()+1000)return;return this.authenticate(!!this.#session,options.signal);}
 private async http(path:string,method:string,body:unknown,session:Session|undefined,signal:AbortSignal,onLate?:(response:Response)=>void):Promise<HTTPResult>{
  try {return await bounded(async active=>{
   const response=await fetch(this.#origin+'/api/v1'+path,{method,redirect:'error',credentials:'omit',cache:'no-store',signal:active,headers:{accept:'application/json',...(body===undefined?{}:{'content-type':'application/json'}),...(session?{authorization:'Bearer '+session.key,'x-cybergrid-epoch':session.epoch}:{})},body:body===undefined?undefined:JSON.stringify(body)});
   if(active.aborted){onLate?.(response);throw active.reason;}
   let data:unknown;try{data=await readJSON(response,active);}catch(e){if(response.ok)throw e;}
   const raw=data&&typeof data==='object'?(data as Record<string,unknown>).error:undefined;
   const code=raw&&typeof raw==='object'?(raw as Record<string,unknown>).code:undefined;
   const retryAfter=response.headers.get('retry-after');const retryAfterMs=retryAfter&&/^\d{1,6}$/.test(retryAfter)?Math.min(Number(retryAfter)*1000,30000):undefined;
   return {retryAfterMs,status:response.status,epoch:response.headers.get('x-cybergrid-epoch'),data,apiCode:typeof code==='string'&&['INVALID_SESSION','COORDINATOR_RESET','SESSION_SUPERSEDED','DIRECTORY_TOO_LARGE','INVALID_IDENTITY'].includes(code)?code:undefined};
  },[signal],this.#timeout);}catch(e){if(e instanceof CyberGridError)throw e;return fail('NETWORK_ERROR');}
 }
 private current(f:Flight){if(f.generation!==this.#generation||f.controller.signal.aborted||this.#flight!==f)return fail('CANCELLED');}
 private async config(value:unknown):Promise<{epoch:string;endpointPolicy:EndpointPolicy;hasPolicy:boolean;products?:ReadonlySet<string>}> {
  const c=object(value),scope=typeof c.scope==='string'?c.scope.split(' '):[];
  if(c.provider!=='logto'||c.issuer!==this.#issuer||c.clientId!==this.#clientId||c.resource!==this.#resource||c.authorizationEndpoint!==this.#issuer+'/auth'||c.tokenEndpoint!==this.#issuer+'/token'||c.revocationEndpoint!==this.#issuer+'/token/revocation'||c.jwksUri!==this.#issuer+'/jwks'||!scope.includes('connect')||scope.length>KNOWN_SCOPES.length||new Set(scope).size!==scope.length||scope.some(v=>!KNOWN_SCOPES.includes(v)))return fail('CONFIGURATION_MISMATCH');
  const hasVersion=Object.hasOwn(c,'policyVersion'),hasProducts=Object.hasOwn(c,'products');
  if(hasVersion!==hasProducts)return fail('INVALID_RESPONSE');
  const products=hasVersion?new Set<string>():undefined;
  if(hasVersion){if(typeof c.policyVersion!=='string'||!/^[a-f0-9]{64}$/.test(c.policyVersion)||!Array.isArray(c.products)||c.products.length<1||c.products.length>32)return fail('INVALID_RESPONSE');const ids=new Set<string>();for(const value of c.products){const product=object(value),id=text(product.id,21),name=text(product.name,256);if(!/^[A-Za-z0-9_-]{1,21}$/.test(id)||ids.has(id)||!name.trim())return fail('INVALID_RESPONSE');ids.add(id);products!.add(id);}}
  const coordination=object(c.coordination);if(coordination.protocol!=='memory-v1'||!uuid(coordination.epoch))return fail('INVALID_RESPONSE');return {epoch:coordination.epoch,products,hasPolicy:c.endpointPolicy!==undefined,endpointPolicy:await policyConfiguration(c.endpointPolicy)};
 }
 private parseSession(result:HTTPResult,epoch:string):Session {
  const d=object(result.data);if(d.purpose!=='console'||d.deviceId!=null||d.appId!=null||typeof d.sessionKey!=='string'||d.sessionKey.length!==43||!/^[A-Za-z0-9_-]{43}$/.test(d.sessionKey)||d.coordinatorEpoch!==epoch||result.epoch!==epoch)return fail('INVALID_RESPONSE');
  const expires=typeof d.expiresAt==='string'?Date.parse(d.expiresAt):NaN;if(!Number.isFinite(expires)||expires<=Date.now()||expires>Date.now()+24*3600*1000+300000)return fail('INVALID_RESPONSE');
  const user=object(d.user);return {key:d.sessionKey,epoch,expires,subject:text(user.id,128)};
 }
 private statusError(result:HTTPResult,stage:ErrorStage):never {const status=result.status,context={stage,retryable:status===429||status>=500,retryAfterMs:result.retryAfterMs};if(status===401)return fail('AUTHENTICATION_FAILED',status,{...context,retryable:false});if(status===403)return fail('FORBIDDEN',status,{...context,retryable:false});return fail('SESSION_UNAVAILABLE',status,context);}
 private async configuration(f:Flight,signal:AbortSignal):Promise<HTTPResult>{
  return bounded(async active=>{for(let attempt=0;;attempt++){
   this.stage(f,'configuration','configuration-request');let response:HTTPResult|undefined,error:unknown;
   try{response=await this.http('/console/auth/config?clientId='+encodeURIComponent(this.#clientId),'GET',undefined,undefined,active);}catch(e){error=e;}
   this.current(f);if(active.aborted)throw active.reason;
   const retry=response?(response.status===429||response.status>=500):error instanceof CyberGridError&&error.code==='NETWORK_ERROR';
   if(!retry||attempt===2){if(error)throw contextual(error,'configuration');return response!;}
   const delay=Math.min(response?.retryAfterMs??100*2**attempt,1000);
   this.stage(f,'configuration','configuration-retry',{retryable:true,retryAfterMs:delay,nextRetryAt:Date.now()+delay});
   await new Promise<void>((resolve,reject)=>{const done=()=>{clearTimeout(timer);active.removeEventListener('abort',cancel);resolve();},cancel=()=>{clearTimeout(timer);active.removeEventListener('abort',cancel);reject(active.reason);},timer=setTimeout(done,delay);active.addEventListener('abort',cancel,{once:true});if(active.aborted)cancel();});
  }},[signal],this.#timeout);
 }
 private async hostToken(f:Flight,signal:AbortSignal,forceRefresh:boolean):Promise<string>{
  this.stage(f,'host-authorization',forceRefresh?'authorization-rejected':'host-token-request',{identity:'requesting'});
  const token=await bounded(async active=>{try{return await this.#tokenProvider(Object.freeze({issuer:this.#issuer,clientId:this.#clientId,resource:this.#resource,scopes:RESOURCE_SCOPES,minValidity:this.#minValidity,forceRefresh,requestReason:forceRefresh?'authorization-rejected':f.reason,generation:f.generation,signal:active}));}catch{return fail('AUTHENTICATION_FAILED',undefined,{stage:'host-authorization'});}},[signal],this.#timeout);
  this.current(f);if(typeof token!=='string'||!token.length||token.length>16384||/[\u0000-\u001f\u007f]/.test(token))return fail('LOGIN_REQUIRED',undefined,{stage:'host-authorization'});return token;
 }
 private authenticate(recover:boolean,signal?:AbortSignal,expected?:Session):Promise<void>{
  if(signal?.aborted)return Promise.reject(new CyberGridError('CANCELLED'));
  if(expected&&this.#session&&this.#session!==expected)return Promise.resolve();
  let f=this.#flight;
  if(!f){
   const controller=new AbortController(),generation=this.#generation;this.#session=undefined;this.#state=recover?'recovering':'connecting';
   f={generation,controller,promise:Promise.resolve(),waiters:0,settled:false,reason:recover?'coordination-recovery':'connect',stage:'configuration'};this.#flight=f;this.update({coordination:this.#state,reason:recover?'coordination-recovery':'connect-requested',stage:'configuration',retryable:false,retryAfterMs:null,nextRetryAt:null,diagnosticId:null});
   const own=f;f.promise=Promise.resolve().then(()=>this.establish(own)).catch(e=>{const base=contextual(e,own.stage),error=own.stage==='exchange'&&['NETWORK_ERROR','TIMEOUT','CANCELLED'].includes(base.code)&&this.#pending?.generation===generation&&this.#pending.unknown?new CyberGridError(base.code,base.status,{stage:'exchange',reason:'exchange-outcome-unknown',retryable:base.retryable,...(base.retryAfterMs===null?{}:{retryAfterMs:base.retryAfterMs})}):base;if(this.#flight===own&&this.#generation===generation){this.#session=undefined;this.#state='disconnected';this.update({coordination:error.retryable||error.code==='SESSION_UNAVAILABLE'?'unavailable':'disconnected',hostBusinessAuthorization:error.code==='IDENTITY_CHANGED'?'unknown':this.#snapshot.hostBusinessAuthorization,identity:error.code==='IDENTITY_CHANGED'?'changed':['LOGIN_REQUIRED','AUTHENTICATION_FAILED'].includes(error.code)?'login-required':this.#subject?'verified':'unknown',stage:error.stage,reason:error.reason,retryable:error.retryable,retryAfterMs:error.retryAfterMs,nextRetryAt:null,diagnosticId:error.diagnosticId});}throw error;}).finally(()=>{own.settled=true;if(this.#flight===own)this.#flight=undefined;});
  }
  return this.join(f,signal);
 }
 private join(f:Flight,signal?:AbortSignal):Promise<void>{
  f.waiters++;
  return new Promise<void>((resolve,reject)=>{
   let done=false;const finish=(error?:unknown)=>{if(done)return;done=true;signal?.removeEventListener('abort',cancel);f.waiters--;if(f.waiters===0&&!f.settled){f.controller.abort();if(this.#flight===f){this.#flight=undefined;this.#state='disconnected';this.update({coordination:'disconnected',identity:this.#subject?'verified':'unknown',reason:'cancelled',stage:f.stage,retryable:false,retryAfterMs:null,nextRetryAt:null,diagnosticId:error instanceof CyberGridError?error.diagnosticId:null});}}if(error)reject(error);else resolve();};
   const cancel=()=>finish(new CyberGridError('CANCELLED',undefined,{stage:f.stage}));signal?.addEventListener('abort',cancel,{once:true});f.promise.then(()=>finish(),finish);if(signal?.aborted)cancel();
  });
 }
 private async establish(f:Flight):Promise<void>{
  const parents=[f.controller.signal,this.#life.signal];
  await bounded(async signal=>{
   const response=await this.configuration(f,signal);this.current(f);if(response.status!==200)this.statusError(response,'configuration');
   const config=await this.config(response.data);this.current(f);if(response.epoch!==config.epoch)return fail('INVALID_RESPONSE');if(this.#policy&&this.#policy.policyVersion!==config.endpointPolicy.policyVersion)for(const close of [...this.#bridgeInvalidators])close();this.#policy=config.endpointPolicy;this.#products=config.products;this.#policyUntrusted=false;this.#hasPolicy=config.hasPolicy;this.update({epoch:config.epoch});
   if(!this.#pending||this.#pending.generation!==f.generation||this.#pending.epoch!==config.epoch)this.#pending={generation:f.generation,epoch:config.epoch,requestId:requestId()};
   const pending=this.#pending;
   if(!pending.token)pending.token=await this.hostToken(f,signal,false);
   if(this.#retired.size)await Promise.all([...this.#retired.values()].map(s=>this.cleanSession(s)));
   this.current(f);
   let exchange!:HTTPResult,completedAttempt!:Attempt;
   for(let refreshed=false;;){
    this.stage(f,'exchange','exchange-request');
    if(this.#uncertain.size+this.#retired.size+(this.#pending?.known?1:0)>=8)return fail('SESSION_UNAVAILABLE',undefined,{stage:'exchange',reason:this.#uncertain.size?'exchange-outcome-unknown':'session-unavailable',retryable:false});
    const hadUnknown=!!pending.unknown,attempt:Attempt={pending};this.#uncertain.add(attempt);pending.unknown=true;
    try{exchange=await this.http('/console/auth/exchange','POST',{accessToken:pending.token,coordinationProtocol:'memory-v1',coordinatorEpoch:config.epoch,requestId:pending.requestId},undefined,signal,r=>{void this.lateExchange(r,pending,attempt);});}
    catch(error){pending.unknown=true;const fixed=contextual(error,'exchange');throw new CyberGridError(fixed.code,fixed.status,{stage:'exchange',reason:'exchange-outcome-unknown',retryable:fixed.retryable});}
    if(exchange.status===200){completedAttempt=attempt;break;}
    this.current(f);
    const definitive=exchange.status>=400&&exchange.status<500;if(definitive){this.#uncertain.delete(attempt);pending.unknown=hadUnknown;}
    if(exchange.status===401&&exchange.apiCode==='INVALID_IDENTITY'&&!pending.unknown&&!refreshed){refreshed=true;pending.token=undefined;pending.token=await this.hostToken(f,signal,true);continue;}
    if(pending.unknown)return fail('SESSION_UNAVAILABLE',exchange.status,{stage:'exchange',reason:'exchange-outcome-unknown',retryable:!definitive,retryAfterMs:exchange.retryAfterMs});
    if(definitive&&this.#pending===pending)this.#pending=undefined;this.statusError(exchange,'exchange');
   }
   let session:Session;try{session=this.parseSession(exchange,config.epoch);}catch(error){pending.unknown=true;throw new CyberGridError('INVALID_RESPONSE',undefined,{stage:'exchange',reason:'exchange-outcome-unknown',retryable:true});}this.#uncertain.delete(completedAttempt);
   if(pending.known&&pending.known.key!==session.key)void this.cleanSession(pending.known);pending.known=undefined;
   if(f.generation!==this.#generation||f.controller.signal.aborted||this.#flight!==f){await this.rememberOrClean(session,pending);return fail('CANCELLED');}
   if(this.#subject!==undefined&&this.#subject!==session.subject){this.#pending=undefined;await this.cleanSession(session);return fail('IDENTITY_CHANGED');}
   this.#subject=session.subject;this.#session=session;this.#pending=undefined;this.#state='connected';this.update({epoch:session.epoch,identity:'verified',coordination:'connected',stage:'idle',reason:'connected',retryable:false,retryAfterMs:null,nextRetryAt:null,diagnosticId:null});
  },parents,this.#timeout*3);
 }
 private async lateExchange(response:Response,pending:Pending,attempt:Attempt):Promise<void>{
  try {if(response.status!==200)return;const result=await bounded(async signal=>({status:response.status,epoch:response.headers.get('x-cybergrid-epoch'),data:await readJSON(response,signal)}),[],Math.min(this.#timeout,2000));const session=this.parseSession(result,pending.epoch);this.#uncertain.delete(attempt);await this.rememberOrClean(session,pending);}catch{/* No raw OAuth, HTTP or storage errors cross this boundary. */}
 }
 private async rememberOrClean(session:Session,pending:Pending):Promise<void>{
  if(pending.generation===this.#generation&&this.#pending===pending){if(pending.known&&pending.known.key!==session.key)void this.cleanSession(pending.known);pending.known=session;return;}
  await this.cleanSession(session);
 }
 private cleanSession(session:Session):Promise<void>{
  if(this.#session?.key===session.key)return Promise.resolve();
  this.#retired.set(session.key,session);
  const existing=this.#cleanups.get(session.key);if(existing)return existing;
  if(this.#cleanups.size>=8)return Promise.resolve();
  const operation=bounded(async signal=>{const result=await this.http('/console/session','DELETE',undefined,session,signal);if([200,401,409].includes(result.status))this.#retired.delete(session.key);},[],Math.min(this.#timeout,2000)).catch(()=>{}).finally(()=>{this.#cleanups.delete(session.key);});this.#cleanups.set(session.key,operation);return operation;
 }
 private async authorized(path:string,signal?:AbortSignal,productId?:string):Promise<unknown>{
  const generation=this.#generation;
  for(let attempt=0;attempt<2;attempt++){
   if(signal?.aborted||generation!==this.#generation)return fail('CANCELLED');
   if(!this.#session){if(!this.#flight)return fail('LOGIN_REQUIRED');await this.join(this.#flight,signal);}
   else if(this.#session.expires<=Date.now()+1000)await this.authenticate(true,signal,this.#session);
   const session=this.#session;if(!session)return fail('LOGIN_REQUIRED');if(productId!==undefined)this.selectedProduct(productId);
   const result=await bounded(s=>this.http(path,'GET',undefined,session,s),[signal,this.#life.signal],this.#timeout);
   if(generation!==this.#generation)return fail('CANCELLED');
   if(this.#session!==session){if(attempt===0)continue;return fail('STALE_OPERATION');}
   if(result.epoch!==session.epoch||result.status===401&&result.apiCode==='INVALID_SESSION'||result.status===409&&result.apiCode==='COORDINATOR_RESET'){
    if(attempt===1)return fail('SESSION_UNAVAILABLE');await this.authenticate(true,signal,session);continue;
   }
   if(result.status===413&&result.apiCode==='DIRECTORY_TOO_LARGE')return fail('DIRECTORY_TOO_LARGE',413);
   if(result.status!==200)this.statusError(result,'directory');return result.data;
  }
  return fail('SESSION_UNAVAILABLE');
 }
 private async observeDirectory<T>(work:()=>Promise<T>):Promise<T>{
  const generation=this.#generation;this.update({stage:'directory',reason:'directory-request',retryable:false,retryAfterMs:null,nextRetryAt:null,diagnosticId:null});
  try{if(generation!==this.#generation)return fail('CANCELLED');const value=await work();if(generation!==this.#generation)return fail('CANCELLED');this.update({coordination:this.#session?'connected':this.#snapshot.coordination,stage:'idle',reason:'directory-ready',retryable:false,retryAfterMs:null,nextRetryAt:null,diagnosticId:null});return value;}
  catch(error){const fixed=contextual(error,error instanceof CyberGridError&&error.stage!=='idle'?error.stage:'directory');if(generation===this.#generation)this.update({coordination:['CANCELLED','INVALID_INPUT','NOT_FOUND'].includes(fixed.code)?this.#snapshot.coordination:'unavailable',stage:fixed.stage,reason:fixed.reason,retryable:fixed.retryable,retryAfterMs:fixed.retryAfterMs,nextRetryAt:null,diagnosticId:fixed.diagnosticId});throw fixed;}
 }
 async apps(options:RequestOptions={}):Promise<Application[]>{
  return this.observeDirectory(async()=>{
  const d=object(await this.authorized('/console/apps',options.signal));if((d.source!=='logto'&&d.source!=='static')||!Array.isArray(d.apps)||d.apps.length>5000)return fail('INVALID_RESPONSE');
  return d.apps.map(v=>{const a=object(v);if(a.type!=='Native'||a.isDeviceFlow!==false)return fail('INVALID_RESPONSE');return {id:text(a.id,21),name:text(a.name,256),type:'Native',isDeviceFlow:false};});
  });
 }
 private selectedProduct(productId:string):void {if(this.#products&&!this.#products.has(productId))return fail('CONFIGURATION_MISMATCH');}
 async devices(appId:string,options:RequestOptions={}):Promise<Device[]>{
  return this.observeDirectory(async()=>{
  query(appId,21);this.selectedProduct(appId);const d=object(await this.authorized('/console/devices?appId='+encodeURIComponent(appId),options.signal,appId));if(this.#policyReload){const reload=this.#policyReload;await bounded(()=>reload,[options.signal,this.#life.signal],this.#timeout)}this.selectedProduct(appId);if(this.#policyUntrusted)return fail('CONFIGURATION_MISMATCH');if(!this.#policy)return fail('CONFIGURATION_MISMATCH');return parseDevices(d.devices,this.#policy).map(({id,name,online,lastSeen,ports})=>({id,name,online,lastSeen,ports:ports.map(p=>({...p}))}));
  });
 }
 async getPortTunnelUrl(appId:string,deviceId:string,portId:string,options:RequestOptions={}):Promise<string|null>{
  return this.observeDirectory(async()=>{
  query(deviceId,96);query(portId,64);const devices=await this.devices(appId,options),device=devices.find(d=>d.id===deviceId);if(!device)return fail('NOT_FOUND');const port=device.ports.find(p=>p.id===portId);if(!port)return fail('NOT_FOUND');return device.online?port.tunnelUrl:null;
  });
 }

 async directory(productId:string,options:RequestOptions&{cursor?:string}={}):Promise<DirectorySnapshot>{return this.observeDirectory(async()=>{
  query(productId,21);if(!/^[A-Za-z0-9_-]{1,21}$/.test(productId)||options.cursor!==undefined&&(typeof options.cursor!=='string'||new TextEncoder().encode(options.cursor).length>512))return fail('INVALID_INPUT');
  this.selectedProduct(productId);const generation=this.#generation,sequence=++this.#directorySequence,start=performance.now();
  const value=object(await this.authorized('/console/directory?appId='+encodeURIComponent(productId)+(options.cursor===undefined?'':'&cursor='+encodeURIComponent(options.cursor)),options.signal,productId));
  if(!this.#session)return fail('LOGIN_REQUIRED');this.selectedProduct(productId);if(!this.#hasPolicy)return fail('CONFIGURATION_MISMATCH');
  if(this.#policyUntrusted||this.#policyReload||value.endpointPolicyVersion!==this.#policy?.policyVersion){
   this.#policyUntrusted=true;
   for(const close of [...this.#bridgeInvalidators])close();
   if(!this.#policyReload){
    const work=bounded(async signal=>{let response:HTTPResult|undefined;for(let attempt=0;attempt<3;attempt++){
     let retryAfter:number|undefined;
     try{response=await this.http('/console/auth/config?clientId='+encodeURIComponent(this.#clientId),'GET',undefined,undefined,signal);if(response.status===200)break;if(response.status!==429&&response.status<500)this.statusError(response,'configuration');retryAfter=response.retryAfterMs;if(attempt===2)this.statusError(response,'configuration');}
     catch(error){if(!(error instanceof CyberGridError)||error.code!=='NETWORK_ERROR'||attempt===2)throw error;}
     await new Promise<void>((resolve,reject)=>{const done=()=>{clearTimeout(timer);signal.removeEventListener('abort',cancel);resolve()},cancel=()=>{clearTimeout(timer);signal.removeEventListener('abort',cancel);reject(signal.reason)},timer=setTimeout(done,Math.min(retryAfter??100*2**attempt,1000));signal.addEventListener('abort',cancel,{once:true});if(signal.aborted)cancel()});
    }
    if(!response||response.status!==200)return fail('CONFIGURATION_MISMATCH');const config=await this.config(response.data);if(generation!==this.#generation||signal.aborted)return fail('STALE_OPERATION');
    if(response.epoch!==config.epoch||config.epoch!==this.#session?.epoch||!config.hasPolicy)return fail('CONFIGURATION_MISMATCH');this.#policy=config.endpointPolicy;this.#products=config.products;this.#policyUntrusted=false;
    },[this.#life.signal],this.#timeout);this.#policyReload=work;work.then(()=>{if(this.#policyReload===work)this.#policyReload=undefined},()=>{if(this.#policyReload===work)this.#policyReload=undefined});
   }
   const reload=this.#policyReload;await bounded(()=>reload,[options.signal,this.#life.signal],this.#timeout);
   if(this.#policy?.policyVersion!==value.endpointPolicyVersion)return fail('CONFIGURATION_MISMATCH');
  }
  if(generation!==this.#generation)return fail('STALE_OPERATION');this.selectedProduct(productId);
  const snapshot=parseDirectory(value,productId,this.#session.epoch,this.#policy!);this.#sampleStarts.set(snapshot,{start,sequence});return snapshot;
 });}
 watchDirectory(productId:string,listener:(event:DirectoryEvent)=>void,options:DirectoryWatchOptions={}):DirectoryWatch{
  if(typeof listener!=='function'||!Number.isSafeInteger(options.intervalMs??2000)||(options.intervalMs??2000)<1000||(options.intervalMs??2000)>30000)return fail('INVALID_INPUT');query(productId,21);
  if(this.#watchSlots>=32)return fail('BRIDGE_CAPACITY_EXCEEDED');this.#watchSlots++;
  const controller=new AbortController(),generation=this.#generation,life=this.#life.signal,interval=options.intervalMs??2000;
  const stop=()=>controller.abort(),parents=[options.signal,life];for(const p of parents){p?.addEventListener('abort',stop,{once:true});if(p?.aborted)stop()}
  let cursor:string|undefined,backoff=interval;const emit=(event:DirectoryEvent)=>{if(controller.signal.aborted||generation!==this.#generation)return;try{listener(event)}catch{}};
  const done=(async()=>{while(!controller.signal.aborted&&generation===this.#generation){
   try{const snapshot=await this.directory(productId,{cursor,signal:controller.signal});if(controller.signal.aborted||generation!==this.#generation)break;cursor=snapshot.revision;backoff=interval;emit({type:snapshot.reset?'reset':'snapshot',eventRevision:++this.#directoryRevision,generation,snapshot});}
   catch(error){if(controller.signal.aborted||generation!==this.#generation)break;cursor=undefined;emit({type:'unknown',eventRevision:++this.#directoryRevision,generation,error:contextual(error,'directory')});backoff=Math.min(30000,Math.max(interval,backoff*2));}
   await new Promise<void>(resolve=>{const done=()=>{clearTimeout(timer);controller.signal.removeEventListener('abort',done);resolve()},timer=setTimeout(done,backoff);controller.signal.addEventListener('abort',done,{once:true});if(controller.signal.aborted)done()});
  }})().finally(()=>{this.#watchSlots--;for(const p of parents)p?.removeEventListener('abort',stop)});return Object.freeze({stop,done});
 }


 async connectBridge<Req,Res,StreamInput=unknown>(productId:string,deviceId:string,bridgeId:string,adapter:TrustedBridgeAdapter<Req,Res,StreamInput>,options:BridgeConnectOptions={}):Promise<SecureBridge<Req,Res,StreamInput>>{
  query(deviceId,96);query(bridgeId,64);if(!/^[A-Za-z0-9_-]{1,64}$/.test(bridgeId))return fail('INVALID_INPUT');
  if(this.#snapshot.hostBusinessAuthorization==='denied')return fail('FORBIDDEN');
  if(this.#bridgeSlots>=16)return fail('BRIDGE_CAPACITY_EXCEEDED');this.#bridgeSlots++;
  let released=false;const release=()=>{if(!released){released=true;this.#bridgeSlots--}};
  return connectControlledBridge({directory:(id,o)=>this.directory(id,o),watch:(id,listener,o)=>this.watchDirectory(id,listener,o),sample:s=>this.#sampleStarts.get(s)!,identity:()=>({generation:this.#generation,userId:this.#subject,epoch:this.#session?.epoch}),onInvalidated:close=>{this.#bridgeInvalidators.add(close);return ()=>this.#bridgeInvalidators.delete(close)},release},productId,deviceId,bridgeId,adapter,options);
 }
 async disconnect():Promise<void>{
  const active=this.#session,known=this.#pending?.known;this.#generation++;this.#life.abort();this.#life=new AbortController();this.#flight?.controller.abort();this.#flight=undefined;this.#session=undefined;this.#pending=undefined;this.#policy=undefined;this.#products=undefined;this.#hasPolicy=false;this.#policyReload=undefined;this.#policyUntrusted=false;this.#subject=undefined;this.#state='disconnected';this.update({epoch:null,identity:'unknown',coordination:'disconnected',hostBusinessAuthorization:'unknown',stage:'disconnect',reason:'disconnected',retryable:false,retryAfterMs:null,nextRetryAt:null,diagnosticId:null});
  if(known&&known.key!==active?.key)await this.cleanSession(known);
  if(active)await this.cleanSession(active);
 }
}
