import {createLocalJWKSet,jwtVerify,type JSONWebKeySet} from 'jose';
import {type Binding,type Dependencies,OwnerFailure} from './ram-owner.ts';
export function jwtChecks(binding:Binding):Pick<Dependencies,'verify'|'validateAccess'>{
 let identitySubject:string|undefined;
 let keys:Promise<ReturnType<typeof createLocalJWKSet>>|undefined;
 let get:()=>Promise<ReturnType<typeof createLocalJWKSet>>=()=>Promise.reject(new OwnerFailure('AUTH_UNAVAILABLE'));
 return {
  verify:(config,transport)=>{
   get=()=>keys??=(async()=>{const cfg=await config();if(cfg.issuer!==binding.issuer)throw new OwnerFailure('BINDING_MISMATCH');const r=await transport(cfg.jwksUri,{method:'GET'});const body=await r.text();if(body.length>131072)throw new OwnerFailure('AUTH_UNAVAILABLE');const jwks:JSONWebKeySet=JSON.parse(body);if(!Array.isArray(jwks.keys)||jwks.keys.length<1||jwks.keys.length>32)throw new OwnerFailure('AUTH_UNAVAILABLE');return createLocalJWKSet(jwks);})().catch(e=>{keys=undefined;throw e;});
   return {verifyIdToken:async token=>{const {payload}=await jwtVerify(token,await get(),{issuer:binding.issuer,audience:binding.clientId,requiredClaims:['sub','exp','iat'],clockTolerance:5});if(typeof payload.sub!=='string'||!payload.sub||typeof payload.iat!=='number'||Math.abs(payload.iat-Date.now()/1000)>300)throw new OwnerFailure('AUTH_UNAVAILABLE');if(identitySubject!==undefined&&identitySubject!==payload.sub)throw new OwnerFailure('AUTH_UNAVAILABLE');identitySubject=payload.sub;}};
  },
  validateAccess:async token=>{const {payload:p}=await jwtVerify(token,await get(),{issuer:binding.issuer,audience:binding.resource,requiredClaims:['sub','exp','iat'],clockTolerance:0});if(typeof p.sub!=='string'||!p.sub||p.sub!==identitySubject||typeof p.exp!=='number'||typeof p.scope!=='string'||(p.client_id??p.azp)!==binding.clientId||(p.client_id&&p.azp&&p.client_id!==p.azp))throw new OwnerFailure('AUTH_UNAVAILABLE');return {subject:p.sub,expiresAt:p.exp,scopes:p.scope.split(/\s+/)};}
 };
}
