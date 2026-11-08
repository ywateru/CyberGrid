import {createServer} from 'node:http';
import {resolve} from 'node:path';
import {createConfiguredCoordinator} from './config.js';
import {createOwnerGateway} from './proxy.js';
import {serveAPI} from './http.js';
import {acquireOwnerLease} from './owner-lease.js';
const bind=process.env.CYBERGRID_BIND??'127.0.0.1',port=Number(process.env.PORT??7400);
if(!Number.isInteger(port)||port<1||port>65535||!bind)throw Error('Explicit valid coordinator bind/port required');
const coordinator=createConfiguredCoordinator();let gateway:ReturnType<typeof createOwnerGateway>;
try{gateway=createOwnerGateway(coordinator.handle);}catch(error){coordinator.close();throw error;}
const lease=await acquireOwnerLease(resolve(process.env.CYBERGRID_OWNER_LOCK??'.cybergrid-owner.lock')).catch(error=>{coordinator.close();throw error;});
const sweep=setInterval(()=>void coordinator.store.cleanup(Date.now()).catch(()=>{}),5000);sweep.unref();
const server=createServer((req,res)=>void serveAPI(req,res,gateway).catch(()=>{if(!res.headersSent)res.writeHead(503,{'content-type':'application/json','cache-control':'no-store'});res.end('{"error":{"code":"COORDINATOR_UNAVAILABLE"}}');}));
let stopping=false;async function close(){if(stopping)return;stopping=true;clearInterval(sweep);coordinator.close();server.close();server.closeAllConnections();await lease.close();}
process.on('SIGTERM',()=>void close());process.on('SIGINT',()=>void close());
lease.onLost(()=>{process.exitCode=1;void close();});server.on('error',()=>{process.exitCode=1;void close();});
server.listen({port,host:bind,exclusive:true},()=>console.log(`CyberGrid memory owner listening on ${bind}:${port}, epoch ${coordinator.store.epoch}`));
