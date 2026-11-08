import {resolve} from 'node:path';
import {createConfiguredCoordinator} from './config.js';
import {coordinatorAuthConfiguration,publicOrigin} from './deployment-config.js';
import {acquireOwnerLease} from './owner-lease.js';
import {createNodeServer} from './node-server.js';

const bind=process.env.CYBERGRID_BIND??'127.0.0.1',port=Number(process.env.PORT??7400);
if(!Number.isInteger(port)||port<1||port>65535||!bind)throw Error('Valid bind/port required');
const origin=publicOrigin(process.env.CYBERGRID_PUBLIC_ORIGIN);
const auth=coordinatorAuthConfiguration(process.env);
const coordinator=createConfiguredCoordinator();
const server=createNodeServer(coordinator.handle,{publicOrigin:origin,issuer:auth.issuer,callbackUri:auth.redirectUri});
const lease=await acquireOwnerLease(resolve(process.env.CYBERGRID_OWNER_LOCK??'.cybergrid-owner.lock')).catch(error=>{coordinator.close();throw error;});
const sweep=setInterval(()=>void coordinator.store.cleanup(Date.now()).catch(()=>{}),5000);sweep.unref();
let stopping=false;
async function close(){
 if(stopping)return;stopping=true;clearInterval(sweep);
 // Wake coordination waiters before terminating connections and releasing flock.
 coordinator.close();server.close();server.closeAllConnections();await lease.close();
}
process.on('SIGTERM',()=>void close());process.on('SIGINT',()=>void close());
lease.onLost(()=>{process.exitCode=1;void close();});
server.on('error',()=>{process.exitCode=1;void close();});
server.listen({port,host:bind,exclusive:true},()=>console.log(`CyberGrid Node service listening on ${bind}:${port}, epoch ${coordinator.store.epoch}`));
