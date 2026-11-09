import {createServer} from 'node:http';
import {constants} from 'node:fs';
import {lstat,open,realpath} from 'node:fs/promises';
import {resolve,join,extname,sep} from 'node:path';
import {fileURLToPath} from 'node:url';
import {serveAPI,type Handler} from './http.js';
import {publicOrigin,consoleCallback} from './deployment-config.js';

export interface WebsiteOptions {
 publicOrigin?:string;
 issuer:string;
 callbackUri:string;
 publicRoot?:string;
 ready?:()=>Promise<void>;
}
const types:Record<string,string>={'.html':'text/html; charset=utf-8','.js':'text/javascript; charset=utf-8','.css':'text/css; charset=utf-8','.json':'application/json; charset=utf-8','.svg':'image/svg+xml','.ico':'image/x-icon','.png':'image/png','.txt':'text/plain; charset=utf-8','.md':'text/plain; charset=utf-8'};
/** Same-process website and API router. No network hop or alternate coordinator. */
export function createWebsiteHandler(api:Handler,options:WebsiteOptions):Handler{
 const origin=publicOrigin(options.publicOrigin),issuer=new URL(options.issuer);
 if(issuer.protocol!=='https:'||issuer.username||issuer.password||issuer.search||issuer.hash)throw Error('Pinned HTTPS Logto issuer required');
 const callback=consoleCallback(options.callbackUri,origin);
 const root=resolve(options.publicRoot??fileURLToPath(new URL('../public/',import.meta.url)));
 const csp="default-src 'self'; script-src 'self'; style-src 'self'; connect-src 'self' "+issuer.origin+"; frame-ancestors 'none'; base-uri 'none'; form-action 'self'";
 const route:Handler=async(request,ip)=>{
  const url=new URL(request.url),path=url.pathname;
  if(path==='/healthz'&&options.ready){try{await options.ready();}catch{return new Response('unavailable\n',{status:503,headers:{'cache-control':'no-store'}});}}
  if(path==='/healthz')return new Response(request.method==='HEAD'?null:'ok\n',{status:['GET','HEAD'].includes(request.method)?200:405,headers:{'content-type':'text/plain; charset=utf-8','cache-control':'no-store','x-content-type-options':'nosniff'}});
  if(path==='/api'||path.startsWith('/api/'))return api(request,ip);
  const headers={'content-security-policy':csp,'x-content-type-options':'nosniff','referrer-policy':'no-referrer','cache-control':'no-store'};
  if(!['GET','HEAD'].includes(request.method))return new Response(null,{status:405,headers:{...headers,allow:'GET, HEAD'}});
  // Both the configured and default callback render the console directly. The
  // browser retains its original query (code/state); no redirect consumes it.
  let relative=decodeURIComponent(path);
  if(path===callback.pathname||path==='/console/callback/'||path==='/console/callback'){
   try{consoleCallback(options.callbackUri,url.origin);}catch{return new Response(null,{status:400,headers});}
   relative='/console/';
  }
  if(['/console','/docs','/admin'].includes(relative))relative+='/';
  if(relative.endsWith('/'))relative+='index.html';
  const parts=relative.slice(1).split('/');
  if(parts.some(p=>!p||p==='.'||p==='..'||p.includes('\\')||p.startsWith('.'))||relative.startsWith('/downloads/'))return new Response(null,{status:404,headers});
  let file:Awaited<ReturnType<typeof open>>|undefined;
  try{
   const canonicalRoot=await realpath(root);let candidate=root;
   // Static deployment files are immutable trusted inputs. Reject every symlink,
   // including intermediate directories, rather than exposing arbitrary trees.
   for(const part of parts){candidate=join(candidate,part);if((await lstat(candidate)).isSymbolicLink())return new Response(null,{status:404,headers});}
   if(!(await realpath(candidate)).startsWith(canonicalRoot+sep))return new Response(null,{status:404,headers});
   file=await open(candidate,constants.O_RDONLY|constants.O_NOFOLLOW|constants.O_NONBLOCK);
   const info=await file.stat();if(!info.isFile()||info.size>(2<<20))return new Response(null,{status:404,headers});
   const contentHeaders={...headers,'content-type':types[extname(candidate)]??'application/octet-stream','content-length':String(info.size)};
   return new Response(request.method==='HEAD'?null:new Uint8Array(await file.readFile()),{headers:contentHeaders});
  }catch{return new Response(null,{status:404,headers});}
  finally{await file?.close();}
 };
 return route;
}
export function createNodeServer(api:Handler,options:WebsiteOptions){
 const route=createWebsiteHandler(api,options),origin=publicOrigin(options.publicOrigin);
 const server=createServer((req,res)=>void serveAPI(req,res,route,origin));
 server.requestTimeout=25000;server.headersTimeout=25000;
 return server;
}
