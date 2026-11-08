import {MemoryStore,type MemoryLimits} from './memory.js';
import {LogtoIdentity} from './identity.js';
import {LogtoDirectory} from './directory.js';
import {coordinatorAuthConfiguration} from './deployment-config.js';
import {createService} from './service.js';
export function createConfiguredCoordinator(env:NodeJS.ProcessEnv=process.env){
 const dev=env.CYBERGRID_DEV_MODE==='1';if(dev&&(env.NODE_ENV==='production'||env.VERCEL))throw Error('CYBERGRID_DEV_MODE is forbidden in production / Vercel');
 if(env.VERCEL)throw Error('Vercel is a stateless proxy; coordinator cannot run here');
 const issuer=env.LOGTO_ISSUER,browser=env.LOGTO_BROWSER_CLIENT_ID,id=env.LOGTO_MANAGEMENT_CLIENT_ID,secret=env.LOGTO_MANAGEMENT_CLIENT_SECRET;
 if(!issuer||!browser||!id||!secret)throw Error('Configure pinned Logto issuer, console client and dedicated management credentials; no identity/directory fallback');
 let limits:Partial<MemoryLimits>={};if(env.CYBERGRID_MEMORY_LIMITS){const v:unknown=JSON.parse(env.CYBERGRID_MEMORY_LIMITS);if(!v||typeof v!=='object'||Array.isArray(v))throw Error('Invalid memory limits');limits=v as Partial<MemoryLimits>;}
 const store=new MemoryStore(limits);
 const config=coordinatorAuthConfiguration(env);
 const handle=createService({store,identity:new LogtoIdentity(config),directory:new LogtoDirectory({issuer,clientId:id,clientSecret:secret}),browserClientId:browser,devMode:dev,adminKey:env.CYBERGRID_ADMIN_KEY});
 return {handle,store,close:()=>store.close()};
}
export function createConfiguredService(env:NodeJS.ProcessEnv=process.env){return createConfiguredCoordinator(env).handle;}
