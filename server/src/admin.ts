import {createHash,timingSafeEqual} from 'node:crypto';
import {ApiError,type AdminOverviewQuery} from './types.js';
/** Only a fixed-size digest is compared; the configured key never enters a DTO. */
export function adminVerifier(key:string|undefined){
 if(key===undefined||key==='')return undefined;
 const bytes=Buffer.byteLength(key);if(bytes<32||bytes>256)throw Error('CYBERGRID_ADMIN_KEY must be 32..256 UTF8 bytes');
 const expected=createHash('sha256').update(key).digest();
 return (authorization:string|null)=>{
  const candidate=authorization?.startsWith('Bearer ')?authorization.slice(7):'';
  const bounded=Buffer.byteLength(candidate)<=256;
  const actual=createHash('sha256').update(bounded?candidate:'').digest();
  const equal=timingSafeEqual(actual,expected);
  return bounded&&equal;
 };
}
export function overviewQuery(params:URLSearchParams):AdminOverviewQuery {
 const integer=(key:string,fallback:number,max=Number.MAX_SAFE_INTEGER)=>{const raw=params.get(key);if(raw===null)return fallback;if(!/^[1-9]\d*$/.test(raw))throw new ApiError(400,'INVALID_INPUT','分页参数须为正整数');const value=Number(raw);if(!Number.isSafeInteger(value)||value>max)throw new ApiError(400,'INVALID_INPUT','分页参数超出范围');return value;};
 const page=integer('page',1),pageSize=integer('pageSize',50,100);if(!Number.isSafeInteger((page-1)*pageSize))throw new ApiError(400,'INVALID_INPUT','分页参数超出范围');
 const appId=params.get('appId'),online=params.get('online');if(appId!==null&&(!appId||appId.length>21||appId.includes('\0')))throw new ApiError(400,'INVALID_INPUT','appId 格式不正确');if(online!==null&&online!=='true'&&online!=='false')throw new ApiError(400,'INVALID_INPUT','online 须为 true 或 false');
 return {page,pageSize,...(appId!==null?{appId}:{}),...(online!==null?{online:online==='true'}:{})};
}
