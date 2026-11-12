import {EndpointPolicy} from './endpoints.js';
import {registrationMode} from './admission.js';
import {type MemoryLimits} from './memory.js';
import {RedisStore,redisTransport} from './redis.js';
import {LogtoIdentity} from './identity.js';
import {LogtoDirectory} from './directory.js';
import {ProductPolicy} from './products.js';
import {coordinatorAuthConfiguration,publicOrigin} from './deployment-config.js';
import {createService} from './service.js';
import {browserOrigins} from './console-cors.js';
export function createConfiguredCoordinator(env:NodeJS.ProcessEnv=process.env){
 const dev=env.CYBERGRID_DEV_MODE==='1';if(dev&&env.NODE_ENV==='production')throw Error('CYBERGRID_DEV_MODE is forbidden in production');
 const issuer=env.LOGTO_ISSUER,browser=env.LOGTO_BROWSER_CLIENT_ID,id=env.LOGTO_MANAGEMENT_CLIENT_ID,secret=env.LOGTO_MANAGEMENT_CLIENT_SECRET;
 if(!issuer||!browser)throw Error('Configure pinned Logto issuer and console client');
 const endpointPolicy=new EndpointPolicy(env.CYBERGRID_ENDPOINT_PROVIDERS);
 const products=env.CYBERGRID_PRODUCTS===undefined?undefined:new ProductPolicy(env.CYBERGRID_PRODUCTS);
 if(!products&&(!id||!secret))throw Error('Configure dedicated Logto management credentials for dynamic directory mode');
 const directory=products??new LogtoDirectory({issuer,clientId:id!,clientSecret:secret!,managementOrigin:env.LOGTO_MANAGEMENT_ORIGIN,managementResource:env.LOGTO_MANAGEMENT_RESOURCE});
 let limits:Partial<MemoryLimits>={};if(env.CYBERGRID_LIMITS){const v:unknown=JSON.parse(env.CYBERGRID_LIMITS);if(!v||typeof v!=='object'||Array.isArray(v))throw Error('Invalid coordination limits');limits=v as Partial<MemoryLimits>;}
 const config=coordinatorAuthConfiguration(env);
 const origin=publicOrigin(env.CYBERGRID_PUBLIC_ORIGIN);
 if(dev&&origin&&!['localhost','127.0.0.1','[::1]'].includes(new URL(origin).hostname))throw Error('CYBERGRID_DEV_MODE requires loopback request origins');
 const origins=browserOrigins(env.CYBERGRID_BROWSER_ORIGINS);
 const store=new RedisStore(redisTransport(env),env.CYBERGRID_REDIS_PREFIX,limits,registrationMode(env.CYBERGRID_REGISTRATION_MODE));
 const handle=createService({store,identity:new LogtoIdentity(config),directory,products,endpointPolicy,browserClientId:browser,devMode:dev,adminKey:env.CYBERGRID_ADMIN_KEY,browserOrigins:origins});
 return {handle,store,close:()=>store.close()};
}
export function createConfiguredService(env:NodeJS.ProcessEnv=process.env){return createConfiguredCoordinator(env).handle;}
