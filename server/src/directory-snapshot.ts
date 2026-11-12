import {createHash,createHmac} from 'node:crypto';
import {ApiError,type Device,type DirectorySample} from './types.js';
export function directoryScope(seed:Uint8Array,epoch:string,userId:string,productId:string,policy:string,projection:'native-v1'|'console-v1'):string{
 const key=createHmac('sha256',seed).update('cybergrid-directory-scope-v1').digest();return createHmac('sha256',key).update(JSON.stringify([epoch,userId,productId,policy,projection])).digest('hex');
}
function canonical(value:unknown):unknown{if(Array.isArray(value))return value.map(canonical);if(value&&typeof value==='object')return Object.fromEntries(Object.entries(value).sort(([a],[b])=>a<b?-1:a>b?1:0).map(([k,v])=>[k,canonical(v)]));return value;}
export function directorySnapshot(sample:DirectorySample,productId:string,policy:string,devices:Device[],cursor:string|null){
 if(cursor!==null&&Buffer.byteLength(cursor)>512)throw new ApiError(400,'INVALID_INPUT','目录 cursor 超过512字节');
 const content=createHash('sha256').update(JSON.stringify(canonical(devices.map(({lastSeen,publishRevision,...d})=>d)))).digest('hex');
 const prefix=`v1.${sample.epoch}.${sample.scopeTag}.`,revision=prefix+content;
 const valid=cursor!==null&&/^v1\.[0-9a-f-]{36}\.[0-9a-f]{64}\.[0-9a-f]{64}$/.test(cursor)&&cursor.startsWith(prefix);
 return {version:1,productId,epoch:sample.epoch,endpointPolicyVersion:policy,revision,sampledAt:new Date(sample.sampledAt).toISOString(),reset:!valid,changed:!valid||cursor!==revision,devices};
}
