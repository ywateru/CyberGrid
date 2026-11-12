const methods=new Map([
 ['/api/v1/console/auth/config','GET'],
 ['/api/v1/console/auth/exchange','POST'],
 ['/api/v1/console/apps','GET'],
 ['/api/v1/console/devices','GET'],
 ['/api/v1/console/directory','GET'],
 ['/api/v1/console/session','DELETE'],
]);
const requestHeaders=new Set(['authorization','content-type','x-cybergrid-epoch']);
function validOrigin(value:string):boolean {
 try {
  const u=new URL(value),loopback=u.hostname==='localhost'||u.hostname==='[::1]'||/^127(?:\.\d{1,3}){3}$/.test(u.hostname);
  return value===u.origin&&!u.username&&!u.password&&(u.protocol==='https:'||u.protocol==='http:'&&loopback);
 }catch{return false;}
}
export function browserOrigins(value:string|undefined):ReadonlySet<string> {
 if(value===undefined||value==='')return new Set();
 if(Buffer.byteLength(value)>4096)throw Error('Invalid CYBERGRID_BROWSER_ORIGINS');
 const origins=value.split(',').map(v=>v.trim());
 if(origins.length>16||origins.some(v=>!validOrigin(v))||new Set(origins).size!==origins.length)throw Error('Invalid CYBERGRID_BROWSER_ORIGINS');
 return new Set(origins);
}
export interface ConsoleCors {crossOrigin:boolean;origin?:string;vary:boolean;preflight?:Response;blocked:boolean}
/** Only the console surface can be read by an explicitly trusted browser origin. */
export function consoleCors(request:Request,allowed:ReadonlySet<string>):ConsoleCors {
 const u=new URL(request.url),path=u.pathname.replace(/\/$/,''),method=methods.get(path),origin=request.headers.get('origin');
 const crossOrigin=origin!==null&&origin!==u.origin;
 const approved=!!origin&&validOrigin(origin)&&allowed.has(origin);
 const result:ConsoleCors={crossOrigin,vary:method!==undefined,blocked:false};
 if(request.method==='OPTIONS'){
  const requested=request.headers.get('access-control-request-method');
  const headers=request.headers.get('access-control-request-headers');
  const names=headers===null?[]:headers.split(',').map(v=>v.trim().toLowerCase());
  if(!approved||requested!==method||!method||names.length>3||new Set(names).size!==names.length||names.some(v=>!requestHeaders.has(v))){result.blocked=true;return result;}
  result.origin=origin!;
  result.preflight=new Response(null,{status:204,headers:{'access-control-allow-methods':method,'access-control-allow-headers':names.join(', '),'access-control-max-age':'300','cache-control':'no-store','vary':'Origin, Access-Control-Request-Method, Access-Control-Request-Headers'}});
  return result;
 }
 if(crossOrigin&&(!approved||request.method!==method)){result.blocked=true;return result;}
 if(request.headers.get('sec-fetch-site')==='cross-site'&&!(crossOrigin&&approved&&request.method===method)){result.blocked=true;return result;}
 if(crossOrigin)result.origin=origin!;
 return result;
}
export function applyConsoleCors(response:Response,policy:ConsoleCors):Response {
 if(policy.vary){const existing=response.headers.get('vary');if(!existing?.split(',').some(v=>v.trim().toLowerCase()==='origin'))response.headers.set('vary',existing?existing+', Origin':'Origin');}
 if(policy.origin&&!policy.blocked){response.headers.set('access-control-allow-origin',policy.origin);response.headers.set('access-control-expose-headers','X-CyberGrid-Epoch, Retry-After');}
 return response;
}
