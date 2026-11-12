import {createHash,createHmac,timingSafeEqual} from 'node:crypto';
import {ApiError} from './types.js';
export type RegistrationMode='legacy-v1'|'finite-v2';
export const ADMISSION_CAPABILITY={version:2,challengeSeconds:120,ticketSeconds:86400} as const;
export interface AdmissionContext {userId:string;productId:string;authClientId:string;keyHash:string}
export interface AdmissionClaim extends AdmissionContext {epoch:string;domain:string;issuedAt:number;notAfter:number;nonce?:string}
export interface AdmissionChallenge {challenge:string;epoch:string;expiresAt:string}
export interface AdmissionTicket {admissionTicket:string;epoch:string;expiresAt:string}
type Kind='challenge'|'ticket';
export function registrationMode(value:string|undefined):RegistrationMode {if(value===undefined)return 'legacy-v1';if(value==='legacy-v1'||value==='finite-v2')return value;throw Error('Invalid CYBERGRID_REGISTRATION_MODE');}
export function admissionError(code:'REGISTRATION_ADMISSION_REQUIRED'|'REGISTRATION_ADMISSION_EXPIRED'|'REGISTRATION_ADMISSION_CONFLICT'|'REGISTRATION_ADMISSION_INVALID'):ApiError {return new ApiError(409,code,code==='REGISTRATION_ADMISSION_REQUIRED'?'注册需要有限期准入票据':code==='REGISTRATION_ADMISSION_EXPIRED'?'注册准入已到期':code==='REGISTRATION_ADMISSION_CONFLICT'?'注册准入内容或域冲突':'注册准入无效');}
export function registrationKey(value:unknown):string {if(typeof value!=='string'||Buffer.from(value,'base64url').length!==32||Buffer.from(value,'base64url').toString('base64url')!==value)throw new ApiError(400,'INVALID_INPUT','registrationKey 须为规范32字节base64url');return value;}
export const keyHash=(key:string)=>createHash('sha256').update(registrationKey(key)).digest('hex');
const uuid=(v:unknown):v is string=>typeof v==='string'&&/^[0-9a-f]{8}-[0-9a-f]{4}-[1-5][0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/i.test(v);
const domain=(v:unknown,zero:boolean):v is string=>typeof v==='string'&&/^(0|[1-9][0-9]{0,15})$/.test(v)&&Number.isSafeInteger(Number(v))&&(zero||Number(v)>0);
function signingKey(seed:Uint8Array,kind:Kind){return createHmac('sha256',seed).update('cybergrid-registration-'+kind+'-v2').digest();}
export function signAdmission(seed:Uint8Array,kind:Kind,claim:AdmissionClaim):string {const fields=[2,claim.epoch,claim.userId,claim.productId,claim.authClientId,claim.keyHash,claim.domain,...(kind==='challenge'?[claim.nonce]:[]),claim.issuedAt,claim.notAfter],body=Buffer.from(JSON.stringify(fields)).toString('base64url');return body+'.'+createHmac('sha256',signingKey(seed,kind)).update(body).digest('base64url');}
/** Verify without a process-time decision: Redis checks expiry at the atomic commit. */
export function verifyAdmission(seed:Uint8Array,kind:Kind,raw:unknown,expected:AdmissionContext,epoch:string):AdmissionClaim {
 try{
  if(typeof raw!=='string'||raw.length>4096)throw Error();const pieces=raw.split('.');if(pieces.length!==2)throw Error();const [body,signature]=pieces as [string,string];
  for(const piece of pieces)if(!/^[A-Za-z0-9_-]+$/.test(piece)||Buffer.from(piece,'base64url').toString('base64url')!==piece)throw Error();
  const mac=Buffer.from(signature,'base64url'),want=createHmac('sha256',signingKey(seed,kind)).update(body).digest();if(mac.length!==want.length||!timingSafeEqual(mac,want))throw Error();
  const bytes=Buffer.from(body,'base64url'),text=bytes.toString('utf8');if(!Buffer.from(text).equals(bytes))throw Error();const a:unknown=JSON.parse(text);if(!Array.isArray(a)||a.length!==(kind==='challenge'?10:9))throw Error();
  const [version,claimEpoch,userId,productId,authClientId,hash,d]=a,issuedAt=a[a.length-2],notAfter=a[a.length-1],nonce=kind==='challenge'?a[7]:undefined;
  if(version!==2||claimEpoch!==epoch||userId!==expected.userId||productId!==expected.productId||authClientId!==expected.authClientId||hash!==expected.keyHash||!domain(d,kind==='challenge')||!uuid(claimEpoch)||(kind==='challenge'&&!uuid(nonce))||!Number.isSafeInteger(issuedAt)||issuedAt<0||!Number.isSafeInteger(notAfter)||notAfter!==issuedAt+(kind==='challenge'?120000:86400000))throw Error();
  return {...expected,epoch,domain:d,issuedAt,notAfter,...(nonce?{nonce}:{})};
 }catch{throw admissionError('REGISTRATION_ADMISSION_INVALID');}
}
