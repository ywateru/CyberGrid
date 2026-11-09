import {createConfiguredCoordinator} from './config.js';
import {coordinatorAuthConfiguration,publicOrigin} from './deployment-config.js';
import {createNodeServer} from './node-server.js';
const bind=process.env.CYBERGRID_BIND??'127.0.0.1',port=Number(process.env.PORT??7400);
if(!Number.isInteger(port)||port<1||port>65535||!bind)throw Error('Valid bind/port required');
const coordinator=createConfiguredCoordinator(),auth=coordinatorAuthConfiguration(process.env);
await coordinator.store.ready?.();
const server=createNodeServer(coordinator.handle,{publicOrigin:publicOrigin(process.env.CYBERGRID_PUBLIC_ORIGIN),issuer:auth.issuer,callbackUri:auth.redirectUri,ready:()=>coordinator.store.ready!()});
let stopping=false;
function close(){if(stopping)return;stopping=true;coordinator.close();server.close();server.closeAllConnections();}
process.on('SIGTERM',close);process.on('SIGINT',close);server.on('error',()=>{process.exitCode=1;close();});
server.listen({port,host:bind},()=>console.log('CyberGrid Node service listening; Redis coordination ready'));
