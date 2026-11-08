import {createRemoteJWKSet,jwtVerify,type JWTVerifyGetKey} from 'jose';
import {ApiError,type AuthConfiguration,type Identity,type VerifiedIdentity} from './types.js';
export class LogtoIdentity implements Identity {
 private readonly keys:JWTVerifyGetKey;
 constructor(private readonly config:AuthConfiguration,private readonly algorithms:string[]=['ES384'],keys?:JWTVerifyGetKey){
  const issuer=new URL(config.issuer);
  if(issuer.protocol!=='https:'||issuer.username||issuer.password||issuer.search||issuer.hash||!issuer.pathname.endsWith('/oidc'))throw new Error('Pinned HTTPS Logto issuer required');
  const resource=new URL(config.resource);if(resource.protocol!=='https:'||resource.username||resource.password||resource.search||resource.hash||algorithms.length!==1||algorithms[0]!=='ES384')throw new Error('Pinned HTTPS API resource and ES384 required');
  this.keys=keys??createRemoteJWKSet(new URL(config.issuer+'/jwks'),{timeoutDuration:5000,cooldownDuration:30_000,cacheMaxAge:600_000});
 }
 configuration(clientId:string){return {...this.config,clientId};}
 async verify(token:string):Promise<VerifiedIdentity>{
  if(token.length>16384||token.split('.').length!==3)throw new ApiError(401,'INVALID_IDENTITY','请通过 Logto 重新授权');
  try{const {payload,protectedHeader}=await jwtVerify(token,this.keys,{issuer:this.config.issuer,audience:this.config.resource,algorithms:this.algorithms,requiredClaims:['sub','iat','exp','client_id','scope'],clockTolerance:5});
   if(typeof payload.sub!=='string'||!payload.sub||payload.sub.length>128||typeof payload.client_id!=='string'||!payload.client_id||payload.client_id.length>21||payload.sub===payload.client_id)throw Error('User identity required');
   if(payload.azp!==undefined&&payload.azp!==payload.client_id)throw Error('Conflicting party');
   if(typeof payload.gty==='string'&&['client_credentials','client-credentials'].includes(payload.gty))throw Error('Machine token');
   if(typeof payload.scope!=='string'||!payload.scope.split(/\s+/).includes('connect'))throw Error('Missing permission');
   if(protectedHeader.typ!==undefined&&!['JWT','at+jwt'].includes(protectedHeader.typ))throw Error('Token type');
   return {id:payload.sub,clientId:payload.client_id,...(typeof payload.email==='string'?{email:payload.email}:{})};
  }catch{throw new ApiError(401,'INVALID_IDENTITY','Logto 授权无效或已过期，请重新授权');}
 }
}
