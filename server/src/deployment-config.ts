import {publicOrigin} from './proxy.js';
import type {AuthConfiguration} from './types.js';
export class DeploymentConfigurationError extends Error {
 readonly code='INVALID_CONSOLE_REDIRECT_URI';
 constructor(message:string){super(message);this.name='DeploymentConfigurationError';}
}
/** Deployment inputs are owner configuration, never request/forwarded headers. */
export function coordinatorAuthConfiguration(env:NodeJS.ProcessEnv):AuthConfiguration {
 const issuer=env.LOGTO_ISSUER,clientId=env.LOGTO_BROWSER_CLIENT_ID;if(!issuer||!clientId)throw Error('Pinned Logto issuer and console client ID required');
 const origin=publicOrigin(env.CYBERGRID_PUBLIC_ORIGIN);
 const resource=env.LOGTO_RESOURCE??origin+'/api';const audience=new URL(resource);
 if(audience.protocol!=='https:'||audience.username||audience.password||audience.search||audience.hash)throw Error('LOGTO_RESOURCE must be a pinned HTTPS identifier without credentials, query or fragment');
 const redirectUri=env.CYBERGRID_CONSOLE_REDIRECT_URI??origin+'/console/callback/';const redirect=new URL(redirectUri);
 if(redirect.pathname==='/'||/^\/(?:api|admin)(?:\/|$)/.test(redirect.pathname))throw new DeploymentConfigurationError('Console callback must use a dedicated path outside /, /api and /admin');
 if(redirect.origin!==origin||redirect.username||redirect.password||redirect.search||redirect.hash)throw Error('Console callback must use the configured public HTTPS origin without query or fragment');
 return {provider:'logto',issuer,clientId,resource,scope:'openid offline_access profile email connect',deviceAuthorizationEndpoint:issuer+'/device/auth',authorizationEndpoint:issuer+'/auth',tokenEndpoint:issuer+'/token',revocationEndpoint:issuer+'/token/revocation',redirectUri,jwksUri:issuer+'/jwks'};
}
