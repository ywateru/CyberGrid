import {createHash,createHmac,randomBytes,randomUUID} from 'node:crypto';
import {createClient} from 'redis';
import {REDIS_SCRIPT} from './redis-script.js';
import {MAX_DEVICE_METADATA_BYTES,uuid,type MemoryLimits} from './memory.js';
import {irohReachability} from './iroh.js';
import {ApiError,type Store,type Session,type DeviceSession,type ConsoleSession,type DeviceRow,type DevicePublication,type Device,type StoredSignal,type Signal,type AdminOverview,type AdminOverviewQuery} from './types.js';
const digest=(v:unknown)=>createHash('sha256').update(JSON.stringify(v)).digest('hex');
export const REDIS_HARD_LIMITS:MemoryLimits={sessions:512,sessionsPerUser:128,devices:256,devicesPerUser:64,signals:1024,signalsPerDevice:128,signalsPerUser:512,signalBytes:2097152,signalBytesPerDevice:1048576,signalBytesPerUser:2097152,rateKeys:2048,idempotency:4096,waiters:128,waitersPerDevice:2,registrationKeys:2048,registrationKeysPerUser:128};
const unavailable=()=>new ApiError(503,'COORDINATOR_UNAVAILABLE','协调存储暂时不可用，请稍后重试',1);
export interface RedisTransport {eval(script:string,key:string,args:string[]):Promise<string>;close():void}
export function redisTransport(env:NodeJS.ProcessEnv):RedisTransport {
 const tcp=env.REDIS_URL,rest=env.UPSTASH_REDIS_REST_URL,token=env.UPSTASH_REDIS_REST_TOKEN;
 if((tcp&&(rest||token))||(!tcp&&!(rest&&token)))throw Error('Configure exactly REDIS_URL or UPSTASH_REDIS_REST_URL and UPSTASH_REDIS_REST_TOKEN');
 if(tcp){const url=new URL(tcp);if(!['redis:','rediss:'].includes(url.protocol)||url.hash)throw Error('Invalid REDIS_URL');
  const client=createClient({url:tcp,socket:{connectTimeout:4000,reconnectStrategy:false},disableOfflineQueue:true});client.on('error',()=>{});let connecting:Promise<unknown>|undefined;
  return {async eval(script,key,args){if(!client.isOpen){connecting??=client.connect().finally(()=>{connecting=undefined;});await connecting;}return String(await client.withCommandOptions({timeout:4500}).eval(script,{keys:[key],arguments:args}));},close(){if(client.isOpen)client.destroy();}};
 }
 const url=new URL(rest!);if(url.protocol!=='https:'||url.username||url.password||url.search||url.hash||url.pathname!=='/')throw Error('UPSTASH_REDIS_REST_URL must be an HTTPS origin');
 return {async eval(script,key,args){const abort=new AbortController(),timer=setTimeout(()=>abort.abort(),4500);timer.unref();try{const response=await fetch(url,{method:'POST',headers:{authorization:'Bearer '+token,'content-type':'application/json'},body:JSON.stringify(['EVAL',script,'1',key,...args]),signal:abort.signal,redirect:'error',credentials:'omit'});if(!response.ok)throw unavailable();const body=await response.json() as {result?:unknown;error?:unknown};if(typeof body.result!=='string'||body.error)throw unavailable();return body.result;}finally{clearTimeout(timer);}},close(){}};
}
const messages:Record<string,string>={INVALID_SESSION:'会话已失效，请重新登录',SESSION_SUPERSEDED:'设备已被新注册代际接管',COORDINATOR_RESET:'协调服务已重置，请重新注册',REGISTRATION_ID_CONFLICT:'注册内容冲突',REGISTRATION_ATTEMPT_EXPIRED:'原注册会话已失效，请增加注册revision',CAPACITY_EXCEEDED:'协调容量已满，请稍后重试',INCARNATION_MISMATCH:'设备代际已变化',STALE_PUBLISH_REVISION:'快照revision过期',PUBLISH_REVISION_CONFLICT:'同revision快照冲突',PEER_UNAVAILABLE:'目标设备不可用',SIGNAL_ID_CONFLICT:'信令ID内容冲突',COORDINATOR_UNAVAILABLE:'协调存储暂时不可用'};
const array=<T>(v:unknown):T[]=>Array.isArray(v)?v as T[]:[];
/** Redis owns all mutable coordination state. Local values are projections only. */
export class RedisStore implements Store {
 #transport:RedisTransport;#prefix:string;#seed='';#closed=false;
 epoch='';revision=0;
 readonly limits:MemoryLimits;
 constructor(transport:RedisTransport,prefix='cybergrid:v1',limits:Partial<MemoryLimits>={}){
  if(!/^[a-zA-Z0-9:_-]{1,80}$/.test(prefix))throw Error('Invalid CYBERGRID_REDIS_PREFIX');this.#prefix='{'+prefix+'}';this.#transport=transport;this.limits={...REDIS_HARD_LIMITS,...limits};for(const [k,v] of Object.entries(this.limits))if(!(k in REDIS_HARD_LIMITS)||!Number.isSafeInteger(v)||v<1||v>REDIS_HARD_LIMITS[k as keyof MemoryLimits])throw Error('Invalid coordination capacity');
 }
 close(){this.#closed=true;this.#seed='';this.#transport.close();}
 async #eval<T>(op:string,args:Record<string,unknown>={}):Promise<T>{
  if(this.#closed)throw unavailable();let out:{value:T;error?:string;status?:number};try{out=JSON.parse(await this.#transport.eval(REDIS_SCRIPT,this.#prefix,[op,JSON.stringify(op==='init'?args:{...args,epoch:this.epoch})])) as typeof out;}catch{throw unavailable();}
  if(out.error){if(!(out.error in messages))throw unavailable();throw new ApiError(out.status??503,out.error,messages[out.error]!,out.error==='CAPACITY_EXCEEDED'?5:undefined);}return out.value;
 }
 async ready(){const meta=await this.#eval<{epoch:string;seed:string;revision:number;limits:MemoryLimits}>('init',{epoch:randomUUID(),seed:randomBytes(32).toString('base64url'),limits:this.limits});if(!meta?.epoch||!meta.seed||JSON.stringify(meta.limits)!==JSON.stringify(this.limits)){
   // Lua cjson object key order differs, compare canonical field values instead.
   if(!meta?.epoch||!meta.seed||Object.entries(this.limits).some(([k,v])=>meta.limits?.[k as keyof MemoryLimits]!==v)||Object.keys(meta.limits??{}).length!==Object.keys(this.limits).length)throw unavailable();
  }this.epoch=meta.epoch;this.revision=meta.revision;this.#seed=meta.seed;}
 async #ensure(){if(!this.epoch)await this.ready();}
 async getRevision(){await this.#ensure();return (await this.#eval<{revision:number}>('meta')).revision;}
 async current(s:Session,_now:number){await this.#ensure();await this.#eval('current',{hash:s.hash});}
 async cleanup(_now:number){await this.#ensure();await this.#eval('cleanup');}
 async register(userId:string,appId:string|null,name:string,key:string|undefined,requestId:string,_now:number,registrationRevision?:unknown,expectedEpoch?:string){await this.#ensure();
  if(!uuid(requestId)|| (appId!==null&&(typeof key!=='string'||Buffer.from(key,'base64url').length!==32||Buffer.from(key,'base64url').toString('base64url')!==key)))throw new ApiError(400,'INVALID_INPUT','无效的注册标识');
  if(appId!==null&&(!Number.isSafeInteger(registrationRevision)||(registrationRevision as number)<1))throw new ApiError(400,'INVALID_INPUT','registrationRevision 须为正安全整数');
  const logical=appId===null?undefined:'d_'+digest([userId,appId,key]).slice(0,48),sessionKey=createHmac('sha256',Buffer.from(this.#seed,'base64url')).update(JSON.stringify([this.epoch,userId,appId,requestId])).digest('base64url');
  const session=await this.#eval<Session>('register',{expectedEpoch:expectedEpoch??this.epoch,id:digest([userId,requestId]),content:digest([userId,appId,name,key??null,registrationRevision??null]),logical,userId,appId,name,requestId,registrationRevision,incarnation:randomUUID(),hash:digestKey(sessionKey)});return {session,sessionKey};
 }
 async session(hash:string){await this.#ensure();return (await this.#eval<Session|null>('session',{hash}))??undefined;}
 async revoke(hash:string){await this.#ensure();await this.#eval('revoke',{hash});}
 // Legacy password-era/non-negotiated registrations are deliberately unavailable.
 async createSession(_s:DeviceSession,_d:DeviceRow):Promise<void>{throw new ApiError(400,'COORDINATION_REQUIRED','请使用 memory-v1 协调协议注册');}
 async createConsoleSession(_s:ConsoleSession):Promise<void>{throw new ApiError(400,'COORDINATION_REQUIRED','请使用 memory-v1 协调协议注册');}
 #device(d:DeviceRow&{online:boolean}){const {sessionHash:_,snapshotDigest:__,...safe}=d as typeof d&{sessionHash:string;snapshotDigest:string};return {...safe,ports:array<DeviceRow['ports'][number]>(d.ports),iroh:d.iroh?{...d.iroh,addresses:array<string>(d.iroh.addresses)}:null};}
 async device(id:string,_now:number,s?:Session){await this.#ensure();const d=await this.#eval<(DeviceRow&{online:boolean})|null>('device',{id,sessionHash:s?.hash});return d?this.#device(d):undefined;}
 async devices(userId:string,appId:string,_now:number,s?:Session):Promise<Device[]>{await this.#ensure();const ds=array<DeviceRow&{online:boolean}>(await this.#eval('devices',{userId,appId,sessionHash:s?.hash}));return ds.map(d=>{const {userId:_,appId:__,lastSeen,...safe}=this.#device(d);return {...safe,lastSeen:new Date(lastSeen).toISOString()};});}
 async heartbeat(_id:string,_update:DevicePublication,_now:number):Promise<void>{throw new ApiError(400,'COORDINATION_REQUIRED','心跳必须绑定会话和revision');}
 async publish(s:DeviceSession,p:DevicePublication,incarnation:unknown,revision:unknown,_now:number){await this.#ensure();const update={ports:p.ports,tunnelUrl:p.tunnelUrl,certificateFingerprint:p.certificateFingerprint,iroh:irohReachability(p.iroh)};
  if(Buffer.byteLength(JSON.stringify(update))>MAX_DEVICE_METADATA_BYTES)throw new ApiError(413,'DEVICE_METADATA_TOO_LARGE','完整设备快照超过24KiB');if(!Number.isSafeInteger(revision)||(revision as number)<1)throw new ApiError(400,'INVALID_INPUT','publishRevision 须为正安全整数');
  await this.#eval('publish',{hash:s.hash,update,incarnation,revision,digest:digest([update.ports,update.tunnelUrl,update.certificateFingerprint,update.iroh])});
 }
 async signal(_value:Omit<StoredSignal,'id'>):Promise<number>{throw new ApiError(400,'COORDINATION_REQUIRED','信令必须绑定会话');}
 async postSignal(s:DeviceSession,value:Omit<StoredSignal,'id'|'expiresAt'|'fromDeviceId'>,messageId:unknown,_now:number){await this.#ensure();if(!uuid(messageId))throw new ApiError(400,'INVALID_INPUT','clientMessageId 须为 UUID');return this.#eval<number>('postSignal',{hash:s.hash,id:digest([s.hash,messageId]),content:digest([value,s.incarnation]),value});}
 async signals(to:string,after:number,_now:number,s?:Session):Promise<Signal[]>{await this.#ensure();return array<Signal>(await this.#eval('signals',{id:to,after,sessionHash:s?.hash}));}
 async rate(key:string,_now:number,limit:number){await this.#ensure();return this.#eval<boolean>('rate',{id:digestKey(key),limit});}
 async wait(s:DeviceSession,after:number,seconds:number,_now:()=>number,signal:AbortSignal){await this.current(s,0);if(!seconds||signal.aborted||(await this.signals(s.deviceId,after,0,s)).length)return;const id=randomUUID();await this.#eval('waitAcquire',{hash:s.hash,id});try{const deadline=Date.now()+seconds*1000;let interval=500;while(!signal.aborted&&Date.now()<deadline){if((await this.signals(s.deviceId,after,0,s)).length)return;await new Promise<void>(resolve=>{const done=()=>{clearTimeout(timer);signal.removeEventListener('abort',done);resolve();},timer=setTimeout(done,Math.min(interval,Math.max(0,deadline-Date.now())));signal.addEventListener('abort',done,{once:true});if(signal.aborted)done();});interval=Math.min(2000,interval*2);}}finally{await this.#eval('waitRelease',{id}).catch(()=>{});}await this.current(s,0);}
 async adminOverview(_now:number,query:AdminOverviewQuery):Promise<AdminOverview>{await this.#ensure();if(!Number.isSafeInteger(query.page)||query.page<1||!Number.isSafeInteger(query.pageSize)||query.pageSize<1||query.pageSize>100||!Number.isSafeInteger((query.page-1)*query.pageSize))throw new ApiError(400,'INVALID_INPUT','分页参数无效');const v=await this.#eval<{now:number;startedAt:number;epoch:string;totals:AdminOverview['totals'];devices:(Omit<AdminOverview['devices'][number],'lastSeen'>&{lastSeen:number})[];total:number}>('admin',{query});return {sampledAt:new Date(v.now).toISOString(),coordinator:{epoch:v.epoch,startedAt:new Date(v.startedAt).toISOString(),uptimeSeconds:Math.max(0,Math.floor((v.now-v.startedAt)/1000))},totals:v.totals,page:{number:query.page,size:query.pageSize,totalDevices:v.total,totalPages:Math.max(1,Math.ceil(v.total/query.pageSize)),hasNext:query.page*query.pageSize<v.total},devices:array<typeof v.devices[number]>(v.devices).map(d=>({...d,lastSeen:new Date(d.lastSeen).toISOString(),ports:array(d.ports)}))};}
}
function digestKey(s:string){return createHash('sha256').update(s).digest('hex');}
