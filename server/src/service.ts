import {requestAuthConfiguration} from './deployment-config.js';
import {irohReachability} from './iroh.js';
import {applyConsoleCors,consoleCors} from './console-cors.js';
import { createHash, randomBytes, randomUUID } from 'node:crypto';
import {adminVerifier,overviewQuery} from './admin.js';
import {eligible} from './directory.js';
import { ApiError, ONLINE_TTL, SESSION_TTL, SIGNAL_TTL, type Identity,type Port,type Store,type ApplicationDirectory,type User } from './types.js';
const MAX_BODY=100_000;
const hash=(s:string)=>createHash('sha256').update(s).digest('hex');
const fail=(message:string):never=>{throw new ApiError(400,'INVALID_INPUT',message);};
function text(v:unknown,name:string,max=128,min=1):string {if(typeof v!=='string'||v.length<min||v.length>max||v.includes('\0'))return fail(`${name} 格式不正确`);return v;}
function object(v:unknown):Record<string,unknown>{if(!v||typeof v!=='object'||Array.isArray(v))return fail('需要 JSON 对象');return v as Record<string,unknown>;}
function appID(v:unknown){if(v===undefined||v==='')throw new ApiError(400,'APP_ID_REQUIRED','请提供真实 Logto Application ID');return text(v,'appId',21);}
function ports(v:unknown):Port[]{if(!Array.isArray(v)||v.length>32)return fail('每个设备最多发布 32 个端口');const ids=new Set<string>();return v.map(item=>{const p=object(item);const id=text(p.id,'端口 ID',64);if(ids.has(id))return fail('端口 ID 不能重复');ids.add(id);if(!Number.isInteger(p.port)||(p.port as number)<1||(p.port as number)>65535)return fail('端口应为 1–65535 的整数');if(p.transport!=='tcp')return fail('当前仅支持 tcp 传输');const type=text(p.type,'端口类型',128),url=tunnel(p.tunnelUrl);return {id,port:p.port as number,type,transport:'tcp',...(url?{tunnelUrl:url}:{})};});}
function tunnel(v:unknown):string|null{if(v===undefined||v===null)return null;const value=text(v,'隧道 URL',512);let u:URL;try{u=new URL(value);}catch{return fail('隧道 URL 无效');}if(u.protocol!=='https:'||!/^([a-z0-9]+-)*[a-z0-9]+\.trycloudflare\.com$/.test(u.hostname)||u.username||u.password||u.port||u.pathname!=='/'||u.search||u.hash)return fail('隧道须使用 https 的 trycloudflare.com 子域根地址');return u.origin;}
function json(data:unknown,status=200){return new Response(JSON.stringify(data),{status,headers:{'content-type':'application/json; charset=utf-8','cache-control':'no-store','x-content-type-options':'nosniff','referrer-policy':'no-referrer'}});}
function deviceDirectory(devices:unknown[]){const body=JSON.stringify({devices});if(Buffer.byteLength(body)>2*1024*1024)throw new ApiError(413,'DIRECTORY_TOO_LARGE','设备目录超过2MiB，请减少设备或公布内容');return new Response(body,{headers:{'content-type':'application/json; charset=utf-8','cache-control':'no-store','x-content-type-options':'nosniff','referrer-policy':'no-referrer'}});}
export interface ServiceOptions {store:Store;identity:Identity;directory:ApplicationDirectory;browserClientId:string;browserOrigins?:ReadonlySet<string>;devMode?:boolean;now?:()=>number;authRateLimit?:number;adminKey?:string}
export function createService(options:ServiceOptions){
 const {store,identity,directory}=options;const verifyAdmin=adminVerifier(options.adminKey);const now=options.now??Date.now;const coordination=()=>({protocol:'memory-v1',epoch:store.epoch,heartbeatSeconds:20,leaseSeconds:65,signalTtlSeconds:120,maxSignalWaitSeconds:15});
 async function application(id:string,existing=false){const app=(await directory.list()).find(a=>a.id===id);if(existing&&(!app||!eligible(app)))throw new ApiError(401,'APPLICATION_UNAVAILABLE','Logto 应用已删除或不再支持网页登录，请重新授权');if(!app)throw new ApiError(404,'APPLICATION_NOT_FOUND','找不到此 Logto Application');if(!eligible(app))throw new ApiError(400,'UNSUPPORTED_APPLICATION_FLOW','应用须为关闭 Device Flow 的 Logto Native Application，请使用官方 SDK 网页登录');return app;}
 const inner=async function handle(request:Request,clientIP='unknown',crossOrigin=false):Promise<Response>{try{
  await store.ready?.();const u=new URL(request.url),time=now();if(options.devMode&&!['localhost','127.0.0.1','[::1]'].includes(u.hostname))throw new ApiError(403,'DEV_LOOPBACK_ONLY','开发模式仅允许 loopback 地址');
  const path=u.pathname.replace(/\/$/,''),method=request.method;if(!path.startsWith('/api/v1/'))throw new ApiError(404,'NOT_FOUND','接口不存在');
  if(path==='/api/v1/admin/apps'||path.startsWith('/api/v1/admin/apps/'))throw new ApiError(410,'APPLICATION_MANAGEMENT_REMOVED','请在 Logto 管理应用，CyberGrid 只读应用目录');
  if(path==='/api/v1/admin/overview'){
   if(method!=='GET')throw new ApiError(405,'METHOD_NOT_ALLOWED','管理后台仅提供只读概览');
   if(!verifyAdmin)throw new ApiError(503,'ADMIN_DISABLED','管理后台未配置密钥');
   await store.cleanup(time);
   if(!await store.rate('admin-ip:'+hash(clientIP),time,30))throw new ApiError(429,'RATE_LIMITED','管理请求过于频繁，请稍后重试',60);
   if(!verifyAdmin(request.headers.get('authorization')))throw new ApiError(401,'INVALID_ADMIN_KEY','管理密钥不正确');
   return json(await store.adminOverview(time,overviewQuery(u.searchParams)));
  }
  if(path==='/api/v1/auth/register'||path==='/api/v1/auth/login')throw new ApiError(410,'AUTH_FLOW_REPLACED','请使用 Logto 浏览器授权与 /auth/exchange；中央服务不接收密码');
  const authConfiguration=(id:string)=>{try{return requestAuthConfiguration(identity.configuration(id),u.origin);}catch{throw new ApiError(400,'INVALID_CONSOLE_REDIRECT_URI','控制台回跳与本次访问 origin 不一致');}};
  if(path==='/api/v1/auth/config'&&method==='GET'){const id=appID(u.searchParams.get('appId')??undefined);await application(id);return json({...authConfiguration(id),loginFlow:'authorization_code',coordination:coordination()});}
  if(path==='/api/v1/console/auth/config'&&method==='GET')return json({...authConfiguration(options.browserClientId),coordination:coordination()});
  await store.cleanup(time);
  let body:Record<string,unknown>={};if(['POST','PUT','PATCH'].includes(method)){
   if(!request.headers.get('content-type')?.toLowerCase().startsWith('application/json'))throw new ApiError(415,'JSON_REQUIRED','请使用 application/json');if(Number(request.headers.get('content-length')??0)>MAX_BODY)throw new ApiError(413,'BODY_TOO_LARGE','请求体过大');
   const reader=request.body?.getReader();let length=0;const chunks:Uint8Array[]=[];if(reader){try{while(true){const part=await reader.read();if(part.done)break;length+=part.value.length;if(length>MAX_BODY){await reader.cancel();throw new ApiError(413,'BODY_TOO_LARGE','请求体过大');}chunks.push(part.value);}}finally{reader.releaseLock();}}
   try{body=object(JSON.parse(Buffer.concat(chunks).toString('utf8')));}catch(e){if(e instanceof ApiError)throw e;return fail('JSON 格式不正确');}
  }
  if(path==='/api/v1/auth/exchange'||path==='/api/v1/console/auth/exchange'){
   if(method!=='POST')throw new ApiError(405,'METHOD_NOT_ALLOWED','请求方法不支持');if(!await store.rate('auth-ip:'+hash(clientIP),time,options.authRateLimit??10))throw new ApiError(429,'RATE_LIMITED','尝试次数过多，请一分钟后再试');
   const verified=await identity.verify(text(body.accessToken,'Logto access token',16384));const user:User={id:verified.id,...(verified.email?{email:verified.email}:{})};const sessionKey=randomBytes(32).toString('base64url'),expiresAt=time+SESSION_TTL;
   if(path==='/api/v1/console/auth/exchange'){
    if(body.appId!==undefined||body.registrationKey!==undefined)return fail('控制台会话不接受目标 appId 或 registrationKey');if(verified.clientId!==options.browserClientId)throw new ApiError(401,'APPLICATION_MISMATCH','请使用独立控制台 SPA 授权');
    if(body.coordinationProtocol!==undefined){if(body.coordinationProtocol!=='memory-v1')return fail('不支持的coordinationProtocol');if(body.coordinatorEpoch!==store.epoch)throw new ApiError(409,'COORDINATOR_RESET','注册epoch已改变');const r=await store.register(user.id,null,'',undefined,text(body.requestId,'requestId',36),now(),undefined,body.coordinatorEpoch as string);return json({purpose:'console',sessionKey:r.sessionKey,expiresAt:new Date(r.session.expiresAt).toISOString(),user,coordinatorEpoch:store.epoch});}
    await store.createConsoleSession({purpose:'console',hash:hash(sessionKey),userId:user.id,expiresAt,appId:null,deviceId:null});return json({purpose:'console',sessionKey,expiresAt:new Date(expiresAt).toISOString(),user});
   }
   const requested=appID(body.appId);if(verified.clientId!==requested||verified.clientId===options.browserClientId)throw new ApiError(401,'APPLICATION_MISMATCH','授权 client_id 必须与真实 Native AppID 完全一致');await application(verified.clientId);
   const name=text(body.deviceName,'设备名称',96),deviceId=randomUUID(),appId=verified.clientId;
   if(body.coordinationProtocol!==undefined){if(body.coordinationProtocol!=='memory-v1')return fail('不支持的coordinationProtocol');if(body.coordinatorEpoch!==store.epoch)throw new ApiError(409,'COORDINATOR_RESET','注册epoch已改变');const r=await store.register(user.id,appId,name,text(body.registrationKey,'registrationKey',43,43),text(body.requestId,'requestId',36),now(),body.registrationRevision,body.coordinatorEpoch as string);return json({purpose:'device',appId,sessionKey:r.sessionKey,expiresAt:new Date(r.session.expiresAt).toISOString(),deviceId:r.session.deviceId,user,coordinatorEpoch:store.epoch,incarnation:r.session.incarnation});}
   await store.createSession({purpose:'device',hash:hash(sessionKey),deviceId,userId:user.id,appId,expiresAt},{id:deviceId,userId:user.id,appId,name,lastSeen:time,ports:[],tunnelUrl:null,certificateFingerprint:null,iroh:null});return json({purpose:'device',appId,sessionKey,expiresAt:new Date(expiresAt).toISOString(),deviceId,user});
  }
  if(method!=='DELETE'&&request.headers.has('x-cybergrid-epoch')&&request.headers.get('x-cybergrid-epoch')!==store.epoch)throw new ApiError(409,'COORDINATOR_RESET','协调服务已重启，请重新注册');
  const authorization=request.headers.get('authorization');if(!authorization?.startsWith('Bearer ')||authorization.length>256)throw new ApiError(401,'INVALID_SESSION','会话已失效，请重新登录');
  const session=await store.session(hash(authorization.slice(7)));if(crossOrigin&&session&&session.purpose!=='console')throw new ApiError(403,'FORBIDDEN','跨站接口仅允许控制台会话');
  if(path==='/api/v1/console/session'&&method==='DELETE'){if(!session||session.expiresAt<=time)throw new ApiError(401,'INVALID_SESSION','会话已失效，请重新登录');if(session.purpose!=='console')throw new ApiError(403,'FORBIDDEN','此接口仅允许控制台会话');await store.revoke(session.hash);return json({ok:true});}
  if(path==='/api/v1/session'&&method==='DELETE'){await store.revoke(hash(authorization.slice(7)));return json({ok:true});}if(!session||session.expiresAt<=time)throw new ApiError(401,'INVALID_SESSION','会话已失效，请重新登录');
  if(path==='/api/v1/session'&&method==='DELETE'){await store.revoke(session.hash);return json({ok:true});}
  await store.current(session,time);if(session.protocol&&request.headers.get('x-cybergrid-epoch')!==store.epoch)throw new ApiError(409,'COORDINATOR_RESET','协调服务epoch不匹配');
  if(!await store.rate('api:'+session.hash,time,360))throw new ApiError(429,'RATE_LIMITED','请求过于频繁，请稍后重试');
  if(session.purpose==='device')await application(session.appId,true);await store.current(session,now());
  if(path==='/api/v1/console/apps'){if(session.purpose!=='console')throw new ApiError(403,'FORBIDDEN','此接口仅供只读控制台会话使用');if(method!=='GET')throw new ApiError(405,'METHOD_NOT_ALLOWED','请求方法不支持');return json({source:'logto',apps:(await directory.list()).filter(eligible).map(a=>({id:a.id,name:a.name,type:'Native',isDeviceFlow:a.isDeviceFlow}))});}
  if(path==='/api/v1/apps'&&method==='GET')return json({source:'logto',apps:(await directory.list()).filter(eligible).map(a=>({id:a.id,name:a.name,type:'Native',isDeviceFlow:a.isDeviceFlow}))});
  if(path==='/api/v1/console/devices'){
   if(session.purpose!=='console')throw new ApiError(403,'FORBIDDEN','此接口仅供只读控制台会话使用');if(method!=='GET')throw new ApiError(405,'METHOD_NOT_ALLOWED','请求方法不支持');const id=appID(u.searchParams.get('appId')??undefined);await application(id);const devices=(await store.devices(session.userId,id,time,session)).map(({iroh,...device})=>device);return deviceDirectory(devices);
  }
  if(session.purpose!=='device')throw new ApiError(403,'FORBIDDEN','控制台会话不能访问设备或信令接口');
  if(path==='/api/v1/devices'&&method==='GET')return deviceDirectory(await store.devices(session.userId,session.appId,time,session));
  if(path==='/api/v1/devices/self'&&method==='PUT'){
   const fingerprint=text(body.certificateFingerprint,'证书 fingerprint',64,64);if(!/^[a-f0-9]{64}$/.test(fingerprint))return fail('证书 fingerprint 须为小写 SHA256 十六进制');await store.publish(session,{ports:ports(body.ports),tunnelUrl:tunnel(body.tunnelUrl),certificateFingerprint:fingerprint,iroh:irohReachability(body.iroh)},body.incarnation,body.publishRevision,now());const d=await store.device(session.deviceId,time,session);if(!d)throw new ApiError(401,'INVALID_SESSION','会话已失效，请重新登录');const {userId,appId,lastSeen,...device}=d;return json({device:{...device,lastSeen:new Date(lastSeen).toISOString()}});
  }
  if(path==='/api/v1/signals'&&method==='POST'){
   const to=text(body.toDeviceId,'目标设备',96),connectionId=text(body.connectionId,'连接 ID',96),kind=body.kind;if(kind!=='offer'&&kind!=='answer'&&kind!=='candidate'&&kind!=='close')return fail('未知信令类型');const payload=text(body.payload,'信令 payload',65536,0);if(Buffer.byteLength(payload)>65536)return fail('信令 payload 超过 65536 字节');const [from,destination]=await Promise.all([store.device(session.deviceId,time,session),store.device(to,time,session)]);if(to===session.deviceId||!from?.online||!destination?.online||destination.userId!==session.userId||destination.appId!==session.appId)throw new ApiError(404,'PEER_UNAVAILABLE','目标设备不可用');return json({...(session.protocol?{coordinatorEpoch:store.epoch}:{}),id:await store.postSignal(session,{toDeviceId:to,connectionId,kind,payload,...(session.protocol?{toIncarnation:text(body.toIncarnation,'目标代际',96)}:{})},body.clientMessageId,now())},201);
  }
  if(path==='/api/v1/signals'&&method==='GET'){
   const v=u.searchParams.get('after')??'0';if(!/^\d+$/.test(v)||!Number.isSafeInteger(Number(v)))return fail('after 必须是非负安全整数');const own=await store.device(session.deviceId,time,session);if(!own?.online)throw new ApiError(409,'HEARTBEAT_REQUIRED','请先更新设备心跳');const wait=u.searchParams.get('wait')??'0';if(!/^\d+$/.test(wait)||Number(wait)>15) return fail('wait 须为0..15秒');await store.wait(session,Number(v),Number(wait),now,request.signal);await store.current(session,now());if(!(await store.device(session.deviceId,now(),session))?.online)throw new ApiError(409,'HEARTBEAT_REQUIRED','请更新设备心跳');const signals=await store.signals(session.deviceId,Number(v),now(),session);return json({signals,cursor:signals.at(-1)?.id??Number(v),...(session.protocol?{coordinatorEpoch:store.epoch,directoryRevision:await store.getRevision?.()??store.revision}:{})});
  }
  throw new ApiError(404,'NOT_FOUND','接口不存在');
 }catch(e){if(e instanceof ApiError){const response=json({error:{code:e.code,message:e.message}},e.status);if(e.retryAfter)response.headers.set('retry-after',String(e.retryAfter));return response;}return json({error:{code:'INTERNAL_ERROR',message:'服务暂时不可用，请稍后重试'}},500);}};
 return async(request:Request,ip?:string)=>{const policy=consoleCors(request,options.browserOrigins??new Set());const response=policy.blocked?json({error:{code:'ORIGIN_REJECTED',message:'不允许跨站请求'}},403):policy.preflight??await inner(request,ip,policy.crossOrigin);response.headers.set('x-cybergrid-epoch',store.epoch);return applyConsoleCors(response,policy);};
}
