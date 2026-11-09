import {randomUUID} from 'node:crypto';
import {ApiError,type Identity,type VerifiedIdentity,type ApplicationDirectory,type Application} from '../src/types.js';
export const A='NativeCaseA',B='NativeCaseB',SPA='ConsoleSPA';
export class FixtureIdentity implements Identity {
 private users=new Map<string,{id:string;email:string}>();private tokens=new Map<string,VerifiedIdentity>();
 configuration(clientId:string){return {provider:'logto' as const,issuer:'https://fixture.invalid/oidc',clientId,resource:'https://cybergrid.aixgg.com/api',scope:'openid connect',deviceAuthorizationEndpoint:'https://fixture.invalid/oidc/device/auth',tokenEndpoint:'https://fixture.invalid/oidc/token',authorizationEndpoint:'https://fixture.invalid/oidc/auth',revocationEndpoint:'https://fixture.invalid/oidc/token/revocation',redirectUri:'/console/callback/',jwksUri:'https://fixture.invalid/oidc/jwks'};}
 async register(email:string,..._unused:unknown[]){const user={id:randomUUID(),email};this.users.set(email,user);return {user};}
 tokenFor(email:string,clientId=A){const user=this.users.get(email);if(!user)return 'unknown';const token=randomUUID();this.tokens.set(token,{...user,clientId});return token;}
 async verify(token:string){const user=this.tokens.get(token);if(!user)throw new ApiError(401,'INVALID_IDENTITY','Logto 授权无效或已过期，请重新授权');return {...user};}
}
export class FixtureDirectory implements ApplicationDirectory {
 apps:Application[]=[{id:A,name:'Demo',type:'Native',isDeviceFlow:false},{id:B,name:'Sync',type:'Native',isDeviceFlow:false},{id:SPA,name:'Console',type:'SPA',isDeviceFlow:false}]; unavailable=false;
 async list(){if(this.unavailable)throw new ApiError(503,'IDENTITY_PROVIDER_UNAVAILABLE','目录不可用');return structuredClone(this.apps);}
}
