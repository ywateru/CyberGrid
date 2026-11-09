import {isIP} from 'node:net';
import {ApiError,type IrohReachability} from './types.js';

export const IROH_MAX_ADDRESSES=8;
export const IROH_MAX_METADATA_BYTES=1024;
const invalid=():never=>{throw new ApiError(400,'INVALID_INPUT','iroh 须为有界 endpointId 与纯数字 socket 地址，不支持 relay 或 URL');};

/** Reachability hints only: never resolve names, open sockets, or accept relay URLs. */
export function irohReachability(value:unknown):IrohReachability|null {
 if(value===undefined||value===null)return null;
 if(typeof value!=='object'||Array.isArray(value))return invalid();
 const data=value as Record<string,unknown>,keys=Object.keys(data);
 if(keys.length!==2||!keys.includes('endpointId')||!keys.includes('addresses'))return invalid();
 if(typeof data.endpointId!=='string'||data.endpointId.length!==64||!/^[a-f0-9]{64}$/.test(data.endpointId))return invalid();
 if(!Array.isArray(data.addresses)||data.addresses.length>IROH_MAX_ADDRESSES)return invalid();
 const addresses:string[]=[],seen=new Set<string>();
 for(const address of data.addresses){
  // Longest numeric socket form is an IPv4-embedded IPv6 literal plus a port.
  if(typeof address!=='string'||address.length>53)return invalid();
  const match=/^([0-9.]+):([0-9]{1,5})$/.exec(address)??/^\[([0-9a-fA-F:.]+)\]:([0-9]{1,5})$/.exec(address);
  if(!match||match[0]!==address)return invalid();
  const host=match[1]!,portText=match[2]!,port=Number(portText),version=address.startsWith('[')?6:4;
  if(isIP(host)!==version||port<1||port>65535||String(port)!==portText)return invalid();
  // URL's numeric IPv6 serializer lets equivalent spellings share a dedup key;
  // the published text/order remains untouched. This does not perform a fetch.
  const canonical=version===6?new URL('http://['+host+']/').hostname:host;
  // IPv4-mapped IPv6 inherits IPv4 scope and must not bypass the scope filter.
  const mapped=/^\[::ffff:([0-9a-f]+):([0-9a-f]+)\]$/.exec(canonical);
  const high=mapped?parseInt(mapped[1]!,16):0,low=mapped?parseInt(mapped[2]!,16):0;
  const ipv4=version===4?host:mapped?[high>>>8,high&255,low>>>8,low&255].join('.'):undefined;
  const firstOctet=Number(ipv4?.split('.')[0]);
  if(ipv4==='0.0.0.0'||firstOctet>=224&&firstOctet<=239||canonical==='[::]'||canonical.startsWith('[ff'))return invalid();
  const key=(ipv4??canonical)+':'+port;
  if(seen.has(key))return invalid();
  seen.add(key);addresses.push(address);
 }
 const result={endpointId:data.endpointId,addresses};
 if(Buffer.byteLength(JSON.stringify(result))>IROH_MAX_METADATA_BYTES)return invalid();
 return result;
}
