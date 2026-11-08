import type {IncomingMessage,ServerResponse} from 'node:http';
export type Handler=(request:Request,clientIP?:string)=>Promise<Response>;
const deadlines=new WeakMap<Request,number>();
/** Adapter-created Requests retain the ingress deadline; a proxy cannot restart it. */
export const requestDeadline=(request:Request)=>deadlines.get(request);
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
export async function serveAPI(req:IncomingMessage,res:ServerResponse,handle:Handler,vercel=false){
 const deadline=Date.now()+25000,abort=new AbortController();
 const timer=setTimeout(()=>abort.abort(new Error('Request deadline exceeded')),Math.max(0,deadline-Date.now()));timer.unref();
 const onClose=()=>{if(!res.writableEnded)abort.abort(new Error('Client disconnected'));};
 const onAborted=()=>abort.abort(new Error('Client disconnected'));
 res.on('close',onClose);req.on('aborted',onAborted);
 const onError=(error:Error)=>abort.abort(error);req.on('error',onError);req.once('close',()=>req.off('error',onError));
 const fail=(code:string,status:number)=>{
  if(res.destroyed||res.writableEnded)return;
  req.pause();res.once('finish',()=>{if(!req.complete)req.destroy();});
  res.writeHead(status,{'content-type':'application/json','cache-control':'no-store','connection':'close'});res.end(JSON.stringify({error:{code,message:status===413?'请求体过大':'协调服务暂不可用'}}));
 };
 try{
  const headers=new Headers();for(const [k,v] of Object.entries(req.headers))if(v!==undefined)headers.set(k,Array.isArray(v)?v.join(','):v);
  const host=req.headers.host??'localhost',protocol=vercel?'https':'http';let raw:Buffer|undefined;
  if(Number(req.headers['content-length']??0)>100000)throw new BodyCapacityError('body capacity exceeded');
  if(!['GET','HEAD'].includes(req.method??'GET')){
   raw=await incoming(req,abort.signal);
   // Some Vercel runtimes consume the stream and supply a pre-parsed body.
   const parsed=(req as IncomingMessage & {body?:unknown}).body;
   if(raw.length===0&&parsed!==undefined)raw=Buffer.isBuffer(parsed)?parsed:Buffer.from(typeof parsed==='string'?parsed:JSON.stringify(parsed));
   if(raw.length>100000)throw new BodyCapacityError('body capacity exceeded');
  }
  abort.signal.throwIfAborted();
  const request=new Request(`${protocol}://${host}${req.url??'/'}`,{method:req.method,headers,body:raw?new Uint8Array(raw):undefined,signal:abort.signal});deadlines.set(request,deadline);
  // Only the platform-set forwarding header is accepted in the hosted adapter.
  const ip=vercel?String(req.headers['x-vercel-forwarded-for']??req.socket.remoteAddress??'unknown').split(',')[0]!:req.socket.remoteAddress??'unknown';
  const response=await abortable(handle(request,ip),abort.signal);
  const bytes=await limited(response.body,2<<20,abort.signal);abort.signal.throwIfAborted();
  res.writeHead(response.status,Object.fromEntries(response.headers));res.end(Buffer.from(bytes));
 }catch(error){fail(error instanceof BodyCapacityError?'BODY_TOO_LARGE':'COORDINATOR_UNAVAILABLE',error instanceof BodyCapacityError?413:503);}
 finally{clearTimeout(timer);res.off('close',onClose);req.off('aborted',onAborted);}
}
