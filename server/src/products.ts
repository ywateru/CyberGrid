import {createHash} from 'node:crypto';
import type {Application,ApplicationDirectory} from './types.js';
export interface Product {id:string;name:string;nativeClientIds:string[];browserClientIds:string[]}
export interface PublicProduct {id:string;name:string}
export const productDigest=(value:unknown)=>createHash('sha256').update(JSON.stringify(value)).digest('hex');
const invalid=()=>new Error('Invalid CYBERGRID_PRODUCTS configuration');
const object=(v:unknown,fields:string[])=>{if(!v||typeof v!=='object'||Array.isArray(v)||Object.keys(v).some(k=>!fields.includes(k)))throw invalid();return v as Record<string,unknown>;};
const id=(v:unknown):string=>{if(typeof v!=='string'||!/^[A-Za-z0-9_-]{1,21}$/.test(v))throw invalid();return v;};
function clients(v:unknown):string[]{if(!Array.isArray(v)||v.length<1||v.length>16)throw invalid();const result=v.map(id);if(new Set(result).size!==result.length)throw invalid();return result.sort();}
/** Fixed deployment policy; static declarations never query Management APIs. */
export class ProductPolicy implements ApplicationDirectory {
 readonly version:string;
 #products:Product[];
 constructor(raw:string){
  if(Buffer.byteLength(raw)>128*1024)throw invalid();let parsed:unknown;try{parsed=JSON.parse(raw);}catch{throw invalid();}
  const config=object(parsed,['version','products']);if(config.version!==1||!Array.isArray(config.products)||config.products.length<1||config.products.length>32)throw invalid();
  const products=config.products.map(value=>{const p=object(value,['id','name','nativeClientIds','browserClientIds']);if(typeof p.name!=='string'||p.name.length<1||p.name.length>256||/[\x00-\x1f\x7f]/.test(p.name))throw invalid();return {id:id(p.id),name:p.name,nativeClientIds:clients(p.nativeClientIds),browserClientIds:clients(p.browserClientIds)};}).sort((a,b)=>a.id<b.id?-1:a.id>b.id?1:0);
  const ids=new Set<string>(),natives=new Set<string>(),browsers=new Set<string>();
  for(const p of products){if(ids.has(p.id)||!p.nativeClientIds.includes(p.id))throw invalid();ids.add(p.id);for(const client of p.nativeClientIds){if(natives.has(client))throw invalid();natives.add(client);}for(const client of p.browserClientIds)browsers.add(client);}
  if([...natives].some(client=>browsers.has(client)))throw invalid();
  this.#products=products;this.version=productDigest({version:1,products});
 }
 native(clientId:string):PublicProduct|undefined {const p=this.#products.find(p=>p.nativeClientIds.includes(clientId));return p?{id:p.id,name:p.name}:undefined;}
 browser(clientId:string):PublicProduct[]|undefined {const result=this.#products.filter(p=>p.browserClientIds.includes(clientId)).map(({id,name})=>({id,name}));return result.length?result:undefined;}
 async list():Promise<Application[]>{return this.#products.map(({id,name})=>({id,name,type:'Native',isDeviceFlow:false}));}
}
