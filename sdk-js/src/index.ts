export type LoginState='disconnected'|'connecting'|'connected'|'recovering';
export interface RequestOptions {signal?:AbortSignal}
export interface AccessTokenRequest {issuer:string;clientId:string;resource:string;scopes:readonly string[];signal:AbortSignal}
export interface ClientOptions {
 baseUrl:string;issuer:string;clientId:string;resource:string;
 getAccessToken:(request:AccessTokenRequest)=>Promise<string|undefined>;
 timeoutMs?:number;
}
export interface Application {id:string;name:string;type:'Native';isDeviceFlow:false}
export interface Port {id:string;port:number;type:string;transport:'tcp';tunnelUrl:string|null}
export interface Device {id:string;name:string;online:boolean;lastSeen:string;ports:Port[]}
export type ErrorCode='INVALID_CONFIGURATION'|'CONFIGURATION_MISMATCH'|'LOGIN_REQUIRED'|'AUTHENTICATION_FAILED'|'SESSION_UNAVAILABLE'|'NETWORK_ERROR'|'TIMEOUT'|'CANCELLED'|'INVALID_RESPONSE'|'FORBIDDEN'|'INVALID_INPUT'|'STALE_OPERATION'|'NOT_FOUND'|'IDENTITY_CHANGED'|'DIRECTORY_TOO_LARGE';
const messages:Record<ErrorCode,string>={INVALID_CONFIGURATION:'Invalid CyberGrid configuration',CONFIGURATION_MISMATCH:'Trusted login configuration does not match',LOGIN_REQUIRED:'Connect using the host Logto login first',AUTHENTICATION_FAILED:'Host login authorization is unavailable',SESSION_UNAVAILABLE:'Console session is temporarily unavailable',NETWORK_ERROR:'CyberGrid request failed',TIMEOUT:'CyberGrid operation timed out',CANCELLED:'CyberGrid operation was cancelled',INVALID_RESPONSE:'CyberGrid returned an invalid response',FORBIDDEN:'Console operation is not permitted',INVALID_INPUT:'Invalid console query',STALE_OPERATION:'An outdated console response was discarded',NOT_FOUND:'The selected device or port was not found',IDENTITY_CHANGED:'Host login identity changed; disconnect before connecting again',DIRECTORY_TOO_LARGE:'The device directory exceeds the supported response size'};
export class CyberGridError extends Error {
 readonly name='CyberGridError';
 constructor(readonly code:ErrorCode,readonly status?:number){super(messages[code]);}
}
const fail=(code:ErrorCode,status?:number):never=>{throw new CyberGridError(code,status);};
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
interface HTTPResult {status:number;epoch:string|null;data:unknown;apiCode?:string}
interface Session {key:string;epoch:string;expires:number;subject:string}
interface Pending {generation:number;epoch:string;requestId:string;known?:Session}
interface Attempt {pending:Pending}
interface Flight {generation:number;controller:AbortController;promise:Promise<void>;waiters:number;settled:boolean}

export class CyberGridClient {
 readonly #origin:string;readonly #issuer:string;readonly #clientId:string;readonly #resource:string;readonly #tokenProvider:ClientOptions['getAccessToken'];readonly #timeout:number;
 #subject?:string;#state:LoginState='disconnected';#generation=0;#life=new AbortController();#session?:Session;#pending?:Pending;#flight?:Flight;#cleanups=new Map<string,Promise<void>>();#uncertain=new Set<Attempt>();#retired=new Map<string,Session>();
 constructor(options:ClientOptions){
  try {
   this.#origin=serviceOrigin(options.baseUrl);const issuer=httpsURL(options.issuer);if(issuer.pathname!=='/oidc'||issuer.href!==options.issuer)throw new CyberGridError('INVALID_CONFIGURATION');
   const resource=httpsURL(options.resource);if(resource.href!==options.resource)throw new CyberGridError('INVALID_CONFIGURATION');
   if(typeof options.clientId!=='string'||!/^[A-Za-z0-9_-]{1,21}$/.test(options.clientId)||typeof options.getAccessToken!=='function')throw new CyberGridError('INVALID_CONFIGURATION');
   this.#timeout=options.timeoutMs??15000;if(!Number.isSafeInteger(this.#timeout)||this.#timeout<100||this.#timeout>30000)throw new CyberGridError('INVALID_CONFIGURATION');
   this.#issuer=options.issuer;this.#clientId=options.clientId;this.#resource=options.resource;this.#tokenProvider=options.getAccessToken;
  }catch{throw new CyberGridError('INVALID_CONFIGURATION');}
 }
 getLoginState():LoginState{return this.#state;}
 async connect(options:RequestOptions={}):Promise<void>{if(options.signal?.aborted)return fail('CANCELLED');if(this.#session&&this.#session.expires>Date.now()+1000)return;return this.authenticate(!!this.#session,options.signal);}
 private async http(path:string,method:string,body:unknown,session:Session|undefined,signal:AbortSignal,onLate?:(response:Response)=>void):Promise<HTTPResult>{
  try {return await bounded(async active=>{
   const response=await fetch(this.#origin+'/api/v1'+path,{method,redirect:'error',credentials:'omit',cache:'no-store',signal:active,headers:{accept:'application/json',...(body===undefined?{}:{'content-type':'application/json'}),...(session?{authorization:'Bearer '+session.key,'x-cybergrid-epoch':session.epoch}:{})},body:body===undefined?undefined:JSON.stringify(body)});
   if(active.aborted){onLate?.(response);throw active.reason;}
   let data:unknown;try{data=await readJSON(response,active);}catch(e){if(response.ok)throw e;}
   const raw=data&&typeof data==='object'?(data as Record<string,unknown>).error:undefined;
   const code=raw&&typeof raw==='object'?(raw as Record<string,unknown>).code:undefined;
   return {status:response.status,epoch:response.headers.get('x-cybergrid-epoch'),data,apiCode:typeof code==='string'&&['INVALID_SESSION','COORDINATOR_RESET','SESSION_SUPERSEDED','DIRECTORY_TOO_LARGE'].includes(code)?code:undefined};
  },[signal],this.#timeout);}catch(e){if(e instanceof CyberGridError)throw e;return fail('NETWORK_ERROR');}
 }
 private current(f:Flight){if(f.generation!==this.#generation||f.controller.signal.aborted||this.#flight!==f)return fail('CANCELLED');}
 private config(value:unknown):{epoch:string} {
  const c=object(value),scope=typeof c.scope==='string'?c.scope.split(' '):[];
  if(c.provider!=='logto'||c.issuer!==this.#issuer||c.clientId!==this.#clientId||c.resource!==this.#resource||c.authorizationEndpoint!==this.#issuer+'/auth'||c.tokenEndpoint!==this.#issuer+'/token'||c.revocationEndpoint!==this.#issuer+'/token/revocation'||c.jwksUri!==this.#issuer+'/jwks'||!scope.includes('connect')||scope.length>KNOWN_SCOPES.length||new Set(scope).size!==scope.length||scope.some(v=>!KNOWN_SCOPES.includes(v)))return fail('CONFIGURATION_MISMATCH');
  const coordination=object(c.coordination);if(coordination.protocol!=='memory-v1'||!uuid(coordination.epoch))return fail('INVALID_RESPONSE');return {epoch:coordination.epoch};
 }
 private parseSession(result:HTTPResult,epoch:string):Session {
  const d=object(result.data);if(d.purpose!=='console'||d.deviceId!=null||d.appId!=null||typeof d.sessionKey!=='string'||d.sessionKey.length!==43||!/^[A-Za-z0-9_-]{43}$/.test(d.sessionKey)||d.coordinatorEpoch!==epoch||result.epoch!==epoch)return fail('INVALID_RESPONSE');
  const expires=typeof d.expiresAt==='string'?Date.parse(d.expiresAt):NaN;if(!Number.isFinite(expires)||expires<=Date.now()||expires>Date.now()+24*3600*1000+300000)return fail('INVALID_RESPONSE');
  const user=object(d.user);return {key:d.sessionKey,epoch,expires,subject:text(user.id,128)};
 }
 private statusError(status:number):never {if(status===401)return fail('AUTHENTICATION_FAILED',status);if(status===403)return fail('FORBIDDEN',status);return fail('SESSION_UNAVAILABLE',status);}
 private authenticate(recover:boolean,signal?:AbortSignal,expected?:Session):Promise<void>{
  if(signal?.aborted)return Promise.reject(new CyberGridError('CANCELLED'));
  if(expected&&this.#session&&this.#session!==expected)return Promise.resolve();
  let f=this.#flight;
  if(!f){
   const controller=new AbortController(),generation=this.#generation;this.#session=undefined;this.#state=recover?'recovering':'connecting';
   f={generation,controller,promise:Promise.resolve(),waiters:0,settled:false};this.#flight=f;
   const own=f;f.promise=Promise.resolve().then(()=>this.establish(own)).catch(e=>{if(this.#flight===own&&this.#generation===generation){this.#session=undefined;this.#state='disconnected';}throw e instanceof CyberGridError?e:new CyberGridError('AUTHENTICATION_FAILED');}).finally(()=>{own.settled=true;if(this.#flight===own)this.#flight=undefined;});
  }
  return this.join(f,signal);
 }
 private join(f:Flight,signal?:AbortSignal):Promise<void>{
  f.waiters++;
  return new Promise<void>((resolve,reject)=>{
   let done=false;const finish=(error?:unknown)=>{if(done)return;done=true;signal?.removeEventListener('abort',cancel);f.waiters--;if(f.waiters===0&&!f.settled){f.controller.abort();if(this.#flight===f){this.#flight=undefined;this.#state='disconnected';}}if(error)reject(error);else resolve();};
   const cancel=()=>finish(new CyberGridError('CANCELLED'));signal?.addEventListener('abort',cancel,{once:true});f.promise.then(()=>finish(),finish);if(signal?.aborted)cancel();
  });
 }
 private async establish(f:Flight):Promise<void>{
  const parents=[f.controller.signal,this.#life.signal];
  await bounded(async signal=>{
   const response=await this.http('/console/auth/config','GET',undefined,undefined,signal);this.current(f);if(response.status!==200)this.statusError(response.status);
   const config=this.config(response.data);if(response.epoch!==config.epoch)return fail('INVALID_RESPONSE');
   if(!this.#pending||this.#pending.generation!==f.generation||this.#pending.epoch!==config.epoch)this.#pending={generation:f.generation,epoch:config.epoch,requestId:requestId()};
   const pending=this.#pending;
   let token:string|undefined;try{token=await bounded(async s=>{try{return await this.#tokenProvider({issuer:this.#issuer,clientId:this.#clientId,resource:this.#resource,scopes:RESOURCE_SCOPES,signal:s});}catch{return fail('AUTHENTICATION_FAILED');}},[signal],this.#timeout);}catch(e){if(e instanceof CyberGridError)throw e;return fail('AUTHENTICATION_FAILED');}
   this.current(f);if(typeof token!=='string'||!token.length||token.length>16384||/[\u0000-\u001f\u007f]/.test(token))return fail('LOGIN_REQUIRED');
   if(this.#retired.size)await Promise.all([...this.#retired.values()].map(s=>this.cleanSession(s)));
   this.current(f);
   if(this.#uncertain.size+this.#retired.size+(this.#pending?.known?1:0)>=8)return fail('SESSION_UNAVAILABLE');
   const attempt:Attempt={pending};this.#uncertain.add(attempt);
   const exchange=await this.http('/console/auth/exchange','POST',{accessToken:token,coordinationProtocol:'memory-v1',coordinatorEpoch:config.epoch,requestId:pending.requestId},undefined,signal,r=>{void this.lateExchange(r,pending,attempt);});
   token=undefined;
   if(exchange.status!==200){if(exchange.status>=400&&exchange.status<500){this.#uncertain.delete(attempt);if(this.#pending===pending)this.#pending=undefined;}this.statusError(exchange.status);}
   const session=this.parseSession(exchange,config.epoch);this.#uncertain.delete(attempt);
   if(pending.known&&pending.known.key!==session.key)void this.cleanSession(pending.known);pending.known=undefined;
   if(f.generation!==this.#generation||f.controller.signal.aborted||this.#flight!==f){await this.rememberOrClean(session,pending);return fail('CANCELLED');}
   if(this.#subject!==undefined&&this.#subject!==session.subject){this.#pending=undefined;await this.cleanSession(session);return fail('IDENTITY_CHANGED');}
   this.#subject=session.subject;this.#session=session;this.#pending=undefined;this.#state='connected';
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
 private async authorized(path:string,signal?:AbortSignal):Promise<unknown>{
  const generation=this.#generation;
  for(let attempt=0;attempt<2;attempt++){
   if(signal?.aborted||generation!==this.#generation)return fail('CANCELLED');
   if(!this.#session){if(!this.#flight)return fail('LOGIN_REQUIRED');await this.join(this.#flight,signal);}
   else if(this.#session.expires<=Date.now()+1000)await this.authenticate(true,signal,this.#session);
   const session=this.#session;if(!session)return fail('LOGIN_REQUIRED');
   const result=await bounded(s=>this.http(path,'GET',undefined,session,s),[signal,this.#life.signal],this.#timeout);
   if(generation!==this.#generation)return fail('CANCELLED');
   if(this.#session!==session){if(attempt===0)continue;return fail('STALE_OPERATION');}
   if(result.epoch!==session.epoch||result.status===401&&result.apiCode==='INVALID_SESSION'||result.status===409&&result.apiCode==='COORDINATOR_RESET'){
    if(attempt===1)return fail('SESSION_UNAVAILABLE');await this.authenticate(true,signal,session);continue;
   }
   if(result.status===413&&result.apiCode==='DIRECTORY_TOO_LARGE')return fail('DIRECTORY_TOO_LARGE',413);
   if(result.status!==200)this.statusError(result.status);return result.data;
  }
  return fail('SESSION_UNAVAILABLE');
 }
 async apps(options:RequestOptions={}):Promise<Application[]>{
  const d=object(await this.authorized('/console/apps',options.signal));if(d.source!=='logto'||!Array.isArray(d.apps)||d.apps.length>5000)return fail('INVALID_RESPONSE');
  return d.apps.map(v=>{const a=object(v);if(a.type!=='Native'||a.isDeviceFlow!==false)return fail('INVALID_RESPONSE');return {id:text(a.id,21),name:text(a.name,256),type:'Native',isDeviceFlow:false};});
 }
 async devices(appId:string,options:RequestOptions={}):Promise<Device[]>{
  const d=object(await this.authorized('/console/devices?appId='+encodeURIComponent(query(appId,21)),options.signal));if(!Array.isArray(d.devices)||d.devices.length>5000)return fail('INVALID_RESPONSE');
  return d.devices.map(value=>{const device=object(value);if(typeof device.online!=='boolean'||!Array.isArray(device.ports)||device.ports.length>32)return fail('INVALID_RESPONSE');const lastSeen=text(device.lastSeen,40);if(!Number.isFinite(Date.parse(lastSeen)))return fail('INVALID_RESPONSE');const ports=device.ports.map(v=>{const p=object(v);if(p.transport!=='tcp'||!Number.isInteger(p.port)||(p.port as number)<1||(p.port as number)>65535)return fail('INVALID_RESPONSE');return {id:text(p.id,64),port:p.port as number,type:text(p.type,128),transport:'tcp' as const,tunnelUrl:portURL(p.tunnelUrl)};});return {id:text(device.id,96),name:text(device.name,96),online:device.online,lastSeen,ports};});
 }
 async getPortTunnelUrl(appId:string,deviceId:string,portId:string,options:RequestOptions={}):Promise<string|null>{
  query(deviceId,96);query(portId,64);const devices=await this.devices(appId,options),device=devices.find(d=>d.id===deviceId);if(!device)return fail('NOT_FOUND');const port=device.ports.find(p=>p.id===portId);if(!port)return fail('NOT_FOUND');return device.online?port.tunnelUrl:null;
 }
 async disconnect():Promise<void>{
  const active=this.#session,known=this.#pending?.known;this.#generation++;this.#life.abort();this.#life=new AbortController();this.#flight?.controller.abort();this.#flight=undefined;this.#session=undefined;this.#pending=undefined;this.#subject=undefined;this.#state='disconnected';
  if(known&&known.key!==active?.key)await this.cleanSession(known);
  if(active)await this.cleanSession(active);
 }
}
