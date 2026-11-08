import type {IncomingMessage,ServerResponse} from 'node:http';
export type Handler=(request:Request,clientIP?:string)=>Promise<Response>;
export class BodyCapacityError extends Error {}
export function abortable<T>(operation:Promise<T>,signal:AbortSignal):Promise<T>{
 return new Promise((resolve,reject)=>{
  const stop=()=>{signal.removeEventListener('abort',stop);reject(signal.reason??new Error('Request canceled'));};
  signal.addEventListener('abort',stop,{once:true});
  operation.then(value=>{signal.removeEventListener('abort',stop);resolve(value);},error=>{signal.removeEventListener('abort',stop);reject(error);});
  if(signal.aborted)stop();
 });
}
/** Cancel pending reads without awaiting a potentially stalled underlying cancel hook. */
export async function limited(body:ReadableStream<Uint8Array>|null,max:number,signal:AbortSignal){
 signal.throwIfAborted();if(!body)return new Uint8Array();
 const reader=body.getReader(),chunks:Uint8Array[]=[];let size=0;
 const cancel=()=>{void reader.cancel(signal.reason).catch(()=>{});};
 signal.addEventListener('abort',cancel,{once:true});
 try{
  for(;;){signal.throwIfAborted();const part=await abortable(reader.read(),signal);signal.throwIfAborted();if(part.done)break;size+=part.value.length;if(size>max){cancel();throw new BodyCapacityError('body capacity exceeded');}chunks.push(part.value);}
 }finally{signal.removeEventListener('abort',cancel);reader.releaseLock();}
 return new Uint8Array(Buffer.concat(chunks));
}
function incoming(req:IncomingMessage,signal:AbortSignal):Promise<Buffer>{
 if(signal.aborted)return Promise.reject(signal.reason);
 if(req.readableEnded)return Promise.resolve(Buffer.alloc(0));
 return new Promise((resolve,reject)=>{
  const parts:Buffer[]=[];let size=0;
  const cleanup=()=>{req.off('data',data);req.off('end',end);req.off('error',error);signal.removeEventListener('abort',stop);};
  const error=(reason:unknown)=>{req.pause();cleanup();reject(reason);};
  const stop=()=>error(signal.reason??new Error('Request canceled'));
  const data=(part:Buffer)=>{size+=part.length;if(size>100000){error(new BodyCapacityError('body capacity exceeded'));return;}parts.push(part);};
  const end=()=>{cleanup();resolve(Buffer.concat(parts));};
  req.on('data',data);req.once('end',end);req.once('error',error);signal.addEventListener('abort',stop,{once:true});if(signal.aborted)stop();
 });
}
export async function serveAPI(req:IncomingMessage,res:ServerResponse,handle:Handler,canonicalOrigin?:string){
 const deadline=Date.now()+25000,abort=new AbortController();
 const timer=setTimeout(()=>abort.abort(new Error('Request deadline exceeded')),Math.max(0,deadline-Date.now()));timer.unref();
 const onClose=()=>{if(!res.writableFinished)abort.abort(new Error('Client disconnected'));};
 const onAborted=()=>abort.abort(new Error('Client disconnected'));
 res.on('close',onClose);req.on('aborted',onAborted);
 const onError=(error:Error)=>abort.abort(error);req.on('error',onError);req.once('close',()=>req.off('error',onError));
 const fail=(code:string,status:number)=>{
  if(res.destroyed)return;
  if(res.headersSent){res.destroy();return;}
  req.pause();res.once('finish',()=>{if(!req.complete)req.destroy();});
  res.writeHead(status,{'content-type':'application/json','cache-control':'no-store','connection':'close'});res.end(JSON.stringify({error:{code,message:status===413?'请求体过大':'协调服务暂不可用'}}));
 };
 try{
  const headers=new Headers();for(const [k,v] of Object.entries(req.headers))if(v!==undefined)headers.set(k,Array.isArray(v)?v.join(','):v);
  const target=req.url??'/';
  if(!validRequestTarget(target)){fail('INVALID_REQUEST_TARGET',400);return;}
  let origin:string;try{origin=effectiveOrigin(req,canonicalOrigin);}catch{fail('INVALID_REQUEST_ORIGIN',400);return;}
  let raw:Buffer|undefined;
  if(Number(req.headers['content-length']??0)>100000)throw new BodyCapacityError('body capacity exceeded');
  if(['GET','HEAD'].includes(req.method??'GET')&&(req.headers['transfer-encoding']||Number(req.headers['content-length']??0)>0)){fail('INVALID_INPUT',400);return;}
  if(!['GET','HEAD'].includes(req.method??'GET')){
   raw=await incoming(req,abort.signal);
   if(raw.length>100000)throw new BodyCapacityError('body capacity exceeded');
  }
  abort.signal.throwIfAborted();
  const request=new Request(origin+target,{method:req.method,headers,body:raw?new Uint8Array(raw):undefined,signal:abort.signal});
  const ip=req.socket.remoteAddress??'unknown';
  const response=await abortable(handle(request,ip),abort.signal);
  const bytes=await limited(response.body,2<<20,abort.signal);abort.signal.throwIfAborted();
  res.writeHead(response.status,Object.fromEntries(response.headers));
  await abortable(new Promise<void>((resolve,reject)=>{
   const done=()=>{cleanup();resolve();},error=(e:Error)=>{cleanup();reject(e);};
   const cleanup=()=>{res.off('finish',done);res.off('error',error);};
   res.once('finish',done);res.once('error',error);res.end(req.method==='HEAD'?undefined:Buffer.from(bytes));
  }),abort.signal);
 }catch(error){fail(error instanceof BodyCapacityError?'BODY_TOO_LARGE':'COORDINATOR_UNAVAILABLE',error instanceof BodyCapacityError?413:503);}
 finally{clearTimeout(timer);res.off('close',onClose);req.off('aborted',onAborted);}
}

/** Validate the unnormalized origin-form target before WHATWG URL dot folding. */
export function validRequestTarget(target:string):boolean {
 if(!target.startsWith('/')||target.startsWith('//')||/[\\\x00-\x20\x7f#]/.test(target))return false;
 try{
  const path=decodeURIComponent(target.split('?')[0]!);
  if(/[\\\x00-\x1f\x7f]/.test(path)||/%(?:2e|2f|5c)/i.test(path))return false;
  if(/%2f|%5c/i.test(target.split('?')[0]!))return false;
  return !path.split('/').some(part=>part==='.'||part==='..');
 }catch{return false;}
}

/** Strict direct/forwarded authority parsing. No network access and no global binding. */
export function effectiveOrigin(req:IncomingMessage,configured?:string):string {
 if(configured)return configured;
 const single=(name:string):string|undefined=>{
  const values:string[]=[];for(let i=0;i<req.rawHeaders.length;i+=2)if(req.rawHeaders[i]!.toLowerCase()===name)values.push(req.rawHeaders[i+1]!);
  if(values.length>1)throw Error('Duplicate origin header');
  const value=values[0];if(value!==undefined&&(!value||value.trim()!==value||/[\x00-\x20\x7f,]/.test(value)))throw Error('Invalid origin header');return value;
 };
 const direct=single('host');if(!direct)throw Error('Host required');
 const forwardedHost=single('x-forwarded-host'),forwardedProto=single('x-forwarded-proto');
 const protocol=forwardedProto??((req.socket as typeof req.socket&{encrypted?:boolean}).encrypted?'https':'http');
 if(protocol!=='http'&&protocol!=='https')throw Error('Invalid origin scheme');
 const authority=(value:string)=>{
  if(value.length>512||/[^\x21-\x7e]|[\\/@?#%,]/.test(value))throw Error('Invalid authority');
  let host:string,port:string|undefined;
  if(value.startsWith('[')){const match=/^(\[[0-9a-fA-F:.]+\])(?::([0-9]+))?$/.exec(value);if(!match)throw Error('Invalid IPv6 authority');host=match[1]!;port=match[2];}
  else{const match=/^([^:]+)(?::([0-9]+))?$/.exec(value);if(!match)throw Error('Invalid authority');host=match[1]!;port=match[2];const domain=host.endsWith('.')?host.slice(0,-1):host;if(domain.length>253||domain.split('.').some(label=>!/^([a-zA-Z0-9]|[a-zA-Z0-9][a-zA-Z0-9-]{0,61}[a-zA-Z0-9])$/.test(label)))throw Error('Invalid hostname');}
  if(port!==undefined&&(port.length>5||Number(port)<1||Number(port)>65535))throw Error('Invalid port');
  const url=new URL(protocol+'://'+value);if(url.username||url.password||url.pathname!=='/'||url.search||url.hash)throw Error('Invalid authority');return url.origin;
 };
 authority(direct);return authority(forwardedHost??direct);
}
