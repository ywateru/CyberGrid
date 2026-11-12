import {ApiError,type Application,type ApplicationDirectory} from './types.js';
const unavailable=()=>new ApiError(503,'IDENTITY_PROVIDER_UNAVAILABLE','Logto 应用目录暂不可用，请稍后重试');
function deviceFlow(metadata:unknown):boolean {if(metadata===undefined)return false;if(!metadata||typeof metadata!=='object'||Array.isArray(metadata))throw unavailable();const value=(metadata as Record<string,unknown>).isDeviceFlow;if(value!==undefined&&typeof value!=='boolean')throw unavailable();return value===true;}
export interface DirectoryOptions {issuer:string;clientId:string;clientSecret:string;managementOrigin?:string;managementResource?:string;fetch?:typeof fetch;now?:()=>number;ttlMs?:number;timeoutMs?:number;maxPages?:number}
export class LogtoDirectory implements ApplicationDirectory {
 private readonly origin:string;private readonly tokenEndpoint:string;private readonly resource:string;private readonly fetcher:typeof fetch;private readonly now:()=>number;private readonly ttl:number;private readonly timeout:number;private readonly maxPages:number;
 private cache:{expires:number;apps:Application[]}|undefined;private pending:Promise<Application[]>|undefined;
 private token:{value:string;expires:number}|undefined;
 constructor(private readonly options:DirectoryOptions){
  let u:URL,origin:URL,resource:URL,resourceValue:string;
  try{
   if([options.issuer,options.managementOrigin,options.managementResource].some(v=>v!==undefined&&/[\x00-\x20\x7f]/.test(v)))throw Error();
   u=new URL(options.issuer);origin=new URL(options.managementOrigin??u.origin);resourceValue=options.managementResource??origin.origin+'/api';resource=new URL(resourceValue);
   if(origin.protocol!=='https:'||origin.username||origin.password||origin.pathname!=='/'||origin.search||origin.hash||resource.protocol!=='https:'||resource.username||resource.password||resource.search||resource.hash)throw Error();
  }catch{throw Error('Dedicated pinned Logto management configuration required');}
  if(u.protocol!=='https:'||u.pathname!=='/oidc'||u.username||u.password||u.search||u.hash||!options.clientId||!options.clientSecret)throw Error('Dedicated pinned Logto management configuration required');
  this.origin=origin.origin;this.tokenEndpoint=u.href+'/token';this.resource=resourceValue;this.fetcher=options.fetch??fetch;this.now=options.now??Date.now;this.ttl=Math.min(30_000,Math.max(0,options.ttlMs??30_000));this.timeout=Math.min(15_000,options.timeoutMs??10_000);this.maxPages=Math.min(50,options.maxPages??50);
 }
 async list():Promise<Application[]>{if(this.cache&&this.cache.expires>this.now())return this.cache.apps.map(a=>({...a}));if(!this.pending)this.pending=this.load().finally(()=>{this.pending=undefined;});return (await this.pending).map(a=>({...a}));}
 private async json(url:string,init:RequestInit,signal:AbortSignal,max=1_000_000){const r=await this.fetcher(url,{...init,signal,redirect:'error'});if(!r.ok)throw unavailable();const reader=r.body?.getReader();let size=0;const chunks:Uint8Array[]=[];try{if(reader)while(true){const p=await reader.read();if(p.done)break;size+=p.value.length;if(size>max){await reader.cancel();throw unavailable();}chunks.push(p.value);}}finally{reader?.releaseLock();}return {data:JSON.parse(Buffer.concat(chunks).toString('utf8')) as unknown,total:r.headers.get('total-number'),bytes:size};}
 private async load():Promise<Application[]>{const started=this.now();const controller=new AbortController();const timer=setTimeout(()=>controller.abort(),this.timeout);try{
  if(!this.token||this.token.expires<=this.now()+10_000){const r=await this.json(this.tokenEndpoint,{method:'POST',headers:{authorization:'Basic '+Buffer.from(this.options.clientId+':'+this.options.clientSecret).toString('base64'),'content-type':'application/x-www-form-urlencoded'},body:new URLSearchParams({grant_type:'client_credentials',resource:this.resource,scope:'all'})},controller.signal,32_768);const t=r.data as Record<string,unknown>;if(typeof t.access_token!=='string'||!t.access_token||t.access_token.length>16384||typeof t.expires_in!=='number'||t.expires_in<=0||(t.scope!==undefined&&t.scope!=='all'))throw unavailable();this.token={value:t.access_token,expires:this.now()+Math.min(t.expires_in,3600)*1000};}
  const apps:Application[]=[];const ids=new Set<string>();let bytes=0;let expectedTotal:number|undefined;
  for(let page=1;page<=this.maxPages;page++){const r=await this.json(this.origin+'/api/applications?page='+page+'&page_size=100',{method:'GET',headers:{authorization:'Bearer '+this.token.value}},controller.signal);bytes+=r.bytes;if(bytes>10_000_000||!Array.isArray(r.data)||r.data.length>100)throw unavailable();for(const raw of r.data){if(!raw||typeof raw!=='object')throw unavailable();const a=raw as Record<string,any>;if(typeof a.id!=='string'||!a.id||a.id.length>21||ids.has(a.id)||typeof a.name!=='string'||a.name.length>256||typeof a.type!=='string')throw unavailable();ids.add(a.id);apps.push({id:a.id,name:a.name,type:a.type,isDeviceFlow:deviceFlow(a.customClientMetadata)});}
   const reported=r.total===null?null:Number(r.total);if(reported!==null&&(!/^\d+$/.test(r.total!)||!Number.isSafeInteger(reported)||(expectedTotal!==undefined&&reported!==expectedTotal)))throw unavailable();if(reported!==null)expectedTotal=reported;const total=expectedTotal??null;if(total!==null&&total<apps.length)throw unavailable();if((total!==null&&apps.length===total)||(total===null&&r.data.length<100)){this.cache={apps,expires:started+this.ttl};return apps;}if(r.data.length===0)throw unavailable();
  }throw unavailable();
 }catch{this.cache=undefined;throw unavailable();}finally{clearTimeout(timer);}}
}
export function eligible(app:Application){return app.type==='Native'&&!app.isDeviceFlow;}
