import {createHash} from 'node:crypto';
import {ApiError,type BrowserBridge,type Device,type DevicePublication} from './types.js';
const invalid=():never=>{throw new ApiError(400,'INVALID_INPUT','入口或桥接元数据不符合受信策略');};
const obj=(v:unknown):Record<string,unknown>=>{if(!v||typeof v!=='object'||Array.isArray(v))return invalid();return v as Record<string,unknown>;};
function keys(v:Record<string,unknown>,expected:string[]){if(Object.keys(v).length!==expected.length||expected.some(k=>!Object.hasOwn(v,k)))invalid();}
function id(v:unknown,max:number,protocol=false):string{if(typeof v!=='string'||v.length<1||v.length>max||!(protocol?/^[A-Za-z0-9_.-]+$/:/^[A-Za-z0-9_-]+$/).test(v))return invalid();return v;}
function path(v:unknown):v is string{return typeof v==='string'&&v.length<=256&&/^\/[A-Za-z0-9_~.!$&'()*+,;=:@/-]*$/.test(v)&&!v.includes('//')&&!v.split('/').some(s=>s==='.'||s==='..');}
function origin(v:unknown):v is string{if(typeof v!=='string'||/[^\x21-\x7e]/.test(v))return false;try{const u=new URL(v);return u.protocol==='https:'&&u.origin===v&&!u.username&&!u.password&&u.pathname==='/'&&!u.search&&!u.hash;}catch{return false;}}
const quick=(u:URL)=>u.hostname.split('.')[0]!.length<=63&&/^([a-z0-9]+-)*[a-z0-9]+\.trycloudflare\.com$/.test(u.hostname)&&!u.port;
export interface EndpointProvider {id:string;origins:string[];paths:string[]}
export class EndpointPolicy {
 readonly providers:EndpointProvider[];readonly policyVersion:string;
 constructor(raw?:string){
  let providers:EndpointProvider[]=[];
  if(raw!==undefined){try{
   if(Buffer.byteLength(raw)>32768)invalid();const v=obj(JSON.parse(raw));keys(v,['version','providers']);if(v.version!==1||!Array.isArray(v.providers)||v.providers.length<1||v.providers.length>16)invalid();
   providers=(v.providers as unknown[]).map(item=>{const p=obj(item);keys(p,['id','origins','paths']);const name=id(p.id,64);if(name==='quick-tunnel')invalid();
    if(!Array.isArray(p.origins)||p.origins.length<1||p.origins.length>16||p.origins.some(x=>!origin(x))||new Set(p.origins).size!==p.origins.length)invalid();
    if(!Array.isArray(p.paths)||p.paths.length<1||p.paths.length>16||p.paths.some(x=>!path(x))||new Set(p.paths).size!==p.paths.length)invalid();
    return {id:name,origins:(p.origins as string[]).slice().sort(),paths:(p.paths as string[]).slice().sort()};
   }).sort((a,b)=>a.id<b.id?-1:a.id>b.id?1:0);if(new Set(providers.map(p=>p.id)).size!==providers.length)invalid();
  }catch{throw Error('Invalid CYBERGRID_ENDPOINT_PROVIDERS');}}
  for(const p of providers){Object.freeze(p.origins);Object.freeze(p.paths);Object.freeze(p);}this.providers=Object.freeze(providers) as unknown as EndpointProvider[];
  this.policyVersion=createHash('sha256').update(JSON.stringify(['cybergrid-endpoint-policy-v1',providers.map(p=>[p.id,p.origins,p.paths])])).digest('hex');
 }
 configuration(){return {version:1 as const,policyVersion:this.policyVersion,providers:structuredClone(this.providers)};}
 private parse(v:unknown,scheme:'https:'|'wss:'):URL{
  if(typeof v!=='string'||v.length<1||v.length>1024||/[^\x21-\x7e]/.test(v)||v.includes('\\')||v.includes('%')||v.includes('?')||v.includes('#'))return invalid();
  let u:URL;try{u=new URL(v);}catch{return invalid();}
  if(u.protocol!==scheme||u.username||u.password||u.search||u.hash||!path(u.pathname))return invalid();
  // Check the original spelling, before URL can hide default ports or dot segments.
  if(v!==u.href&&!(scheme==='https:'&&u.pathname==='/'&&v===u.origin))return invalid();return u;
 }
 tunnel(v:unknown,device=false):string|null{
  if(v===undefined||v===null)return null;const u=this.parse(v,'https:');if(device&&u.pathname!=='/')return invalid();
  if(quick(u)&&u.pathname==='/')return u.origin;
  if(!this.providers.some(p=>p.origins.includes(u.origin)&&p.paths.includes(u.pathname)))return invalid();
  return u.pathname==='/'?u.origin:u.href;
 }
 bridge(v:unknown,providerId:string):string{
  const u=this.parse(v,'wss:');if(providerId==='quick-tunnel'){if(!quick(u))return invalid();return u.href;}
  const p=this.providers.find(p=>p.id===providerId),https='https://'+u.host;
  if(!p||!p.origins.includes(https)||!p.paths.includes(u.pathname))return invalid();return u.href;
 }
 filter(devices:Device[],now:number,consoleProjection=false):Device[]{
  return devices.map(d=>{
   // A whitelist also prevents internal Redis history and hashes from escaping.
   const safe:Device={id:d.id,name:d.name,lastSeen:d.lastSeen,online:d.online,ports:d.ports.map(p=>{let url:string|null=null;try{url=this.tunnel(p.tunnelUrl);}catch{}return {id:p.id,port:p.port,type:p.type,transport:p.transport,...(url?{tunnelUrl:url}:{})};}),tunnelUrl:null,certificateFingerprint:d.certificateFingerprint,...(d.incarnation!==undefined?{incarnation:d.incarnation}:{}),...(d.publishRevision!==undefined?{publishRevision:d.publishRevision}:{}),iroh:consoleProjection?null:d.iroh};
   try{safe.tunnelUrl=this.tunnel(d.tunnelUrl,true);}catch{}
   safe.bridges=(d.bridges??[]).flatMap(b=>{try{const normalized=bridges([b],this);return Date.parse(b.expiresAt)>now?normalized:[];}catch{return [];}}).sort((a,b)=>a.id<b.id?-1:a.id>b.id?1:0);
   if(consoleProjection)delete (safe as Partial<Device>).iroh;return safe;
  }).sort((a,b)=>a.id<b.id?-1:a.id>b.id?1:0);
 }
}
export function bridges(value:unknown,policy?:EndpointPolicy):BrowserBridge[]{
 if(value===undefined)return [];if(!Array.isArray(value)||value.length>4)return invalid();const ids=new Set<string>();
 return value.map(item=>{const b=obj(item);keys(b,['id','protocol','providerId','url','installationId','publicKey','addressRevision','expiresAt','limits']);const name=id(b.id,64);if(ids.has(name))invalid();ids.add(name);const providerId=id(b.providerId,64),protocol=id(b.protocol,128,true),installationId=id(b.installationId,128);
  if(typeof b.publicKey!=='string'||Buffer.from(b.publicKey,'base64url').length!==32||Buffer.from(b.publicKey,'base64url').toString('base64url')!==b.publicKey)invalid();
  if(!Number.isSafeInteger(b.addressRevision)||(b.addressRevision as number)<1)invalid();
  if(typeof b.expiresAt!=='string'||!Number.isFinite(Date.parse(b.expiresAt))||new Date(b.expiresAt).toISOString()!==b.expiresAt)invalid();
  const l=obj(b.limits);keys(l,['maxFrameBytes','maxStreams','maxPendingRequests']);for(const [k,max] of [['maxFrameBytes',65535],['maxStreams',32],['maxPendingRequests',128]] as const)if(!Number.isSafeInteger(l[k])||(l[k] as number)<1||(l[k] as number)>max)invalid();
  if(typeof b.url!=='string'||b.url.length>1024)invalid();if(!policy){try{const u=new URL(b.url as string);if(u.protocol!=='wss:'||u.href!==b.url||u.username||u['password']||/[?#%\\\s]/.test(b.url as string)||!path(u.pathname))invalid();}catch{return invalid();}}const url=policy?policy.bridge(b.url,providerId):b.url as string;
  return {id:name,protocol,providerId,url,installationId,publicKey:b.publicKey as string,addressRevision:b.addressRevision as number,expiresAt:b.expiresAt as string,limits:{maxFrameBytes:l.maxFrameBytes as number,maxStreams:l.maxStreams as number,maxPendingRequests:l.maxPendingRequests as number}};
 }).sort((a,b)=>a.id<b.id?-1:a.id>b.id?1:0);
}
export interface BridgeHistory {id:string;revision:number;identity:string;expires:number}
export function bridgeRecords(bs:BrowserBridge[]):BridgeHistory[]{return bs.map(({expiresAt,addressRevision,...b})=>({id:b.id,revision:addressRevision,identity:createHash('sha256').update(JSON.stringify(b)).digest('hex'),expires:Date.parse(expiresAt)}));}
export function checkBridgeHistory(previous:BridgeHistory[],next:BridgeHistory[],now:number):BridgeHistory[]{
 const history=new Map(previous.map(b=>[b.id,b]));for(const b of next){const old=history.get(b.id);if(b.expires<=now||b.expires>now+300000||old&&(b.revision<old.revision||b.revision===old.revision&&(b.identity!==old.identity||b.expires<old.expires)))invalid();history.set(b.id,b);}if(history.size>4)invalid();return [...history.values()];
}
export function publicationDigest(v:DevicePublication):string{const a:unknown[]=[v.ports,v.tunnelUrl,v.certificateFingerprint,v.iroh??null];if(v.bridges?.length)a.push(v.bridges);return createHash('sha256').update(JSON.stringify(a)).digest('hex');}
