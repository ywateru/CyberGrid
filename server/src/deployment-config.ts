import type {AuthConfiguration} from './types.js';
export const DEFAULT_RESOURCE='https://cybergrid.local/api';
export const DEFAULT_CALLBACK='/console/callback/';
export function publicOrigin(raw:string|undefined):string|undefined {
 if(raw===undefined||raw==='')return undefined;
 if(raw.trim()!==raw)throw Error('CYBERGRID_PUBLIC_ORIGIN must be a canonical HTTPS origin');
 const u=new URL(raw);
 if(u.protocol!=='https:'||u.username||u.password||u.search||u.hash||u.pathname!=='/')throw Error('Canonical public HTTPS origin required');
 return u.origin;
}
export class DeploymentConfigurationError extends Error {
 readonly code='INVALID_CONSOLE_REDIRECT_URI';
 constructor(message:string){super(message);this.name='DeploymentConfigurationError';}
}
/** Only the internal default may be a relative template. Explicit callbacks are HTTPS. */
export function consoleCallback(value:string,origin?:string):URL {
 const template=value===DEFAULT_CALLBACK;
 const redirect=new URL(value,origin??'https://cybergrid.local');
 if((!template&&redirect.protocol!=='https:')||redirect.username||redirect.password||redirect.search||redirect.hash||redirect.pathname==='/'||/^\/(?:api|admin|healthz)(?:\/|$)/.test(redirect.pathname)||(origin&&redirect.origin!==origin))throw new DeploymentConfigurationError('Console callback must use a dedicated same-origin path');
 return redirect;
}
/** Immutable startup identity inputs. Request-derived addresses never select audience. */
export function coordinatorAuthConfiguration(env:NodeJS.ProcessEnv):AuthConfiguration {
 const issuer=env.LOGTO_ISSUER,clientId=env.LOGTO_BROWSER_CLIENT_ID;if(!issuer||!clientId)throw Error('Pinned Logto issuer and console client ID required');
 const origin=publicOrigin(env.CYBERGRID_PUBLIC_ORIGIN);
 const resource=env.LOGTO_RESOURCE??(origin?origin+'/api':DEFAULT_RESOURCE);const audience=new URL(resource);
 if(audience.protocol!=='https:'||audience.username||audience.password||audience.search||audience.hash)throw Error('LOGTO_RESOURCE must be a pinned HTTPS identifier without credentials, query or fragment');
 const redirectUri=env.CYBERGRID_CONSOLE_REDIRECT_URI??(origin?origin+DEFAULT_CALLBACK:DEFAULT_CALLBACK);
 if(env.CYBERGRID_CONSOLE_REDIRECT_URI!==undefined&&new URL(env.CYBERGRID_CONSOLE_REDIRECT_URI).protocol!=='https:')throw new DeploymentConfigurationError('Explicit console callback must be HTTPS');
 consoleCallback(redirectUri,origin);
 return {provider:'logto',issuer,clientId,resource,scope:'openid offline_access profile email connect',deviceAuthorizationEndpoint:issuer+'/device/auth',authorizationEndpoint:issuer+'/auth',tokenEndpoint:issuer+'/token',revocationEndpoint:issuer+'/token/revocation',redirectUri,jwksUri:issuer+'/jwks'};
}
/** Projection only; never mutate the identity's fixed configuration or cache a host. */
export function requestAuthConfiguration(config:AuthConfiguration,origin:string):AuthConfiguration {
 return {...config,redirectUri:consoleCallback(config.redirectUri,origin).href};
}
