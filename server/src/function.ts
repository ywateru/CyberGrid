import type {IncomingMessage,ServerResponse} from 'node:http';
import {resolve} from 'node:path';
import {createConfiguredCoordinator} from './config.js';
import {coordinatorAuthConfiguration,publicOrigin} from './deployment-config.js';
import {createWebsiteHandler} from './node-server.js';
import {serveAPI} from './http.js';
// A warm function may reuse connections; all authorization/state remain in Redis.
let handle:ReturnType<typeof createWebsiteHandler>|undefined;
export default async function handler(req:IncomingMessage,res:ServerResponse){
 try{
  const origin=publicOrigin(process.env.CYBERGRID_PUBLIC_ORIGIN);
  if(!handle){const coordinator=createConfiguredCoordinator(),auth=coordinatorAuthConfiguration(process.env);handle=createWebsiteHandler(coordinator.handle,{publicOrigin:origin,issuer:auth.issuer,callbackUri:auth.redirectUri,publicRoot:resolve(process.cwd(),'server/public'),ready:()=>coordinator.store.ready!()});}
  await serveAPI(req,res,(request)=>handle!(request,'vercel-shared'),origin);
 }catch{if(!res.headersSent){res.writeHead(503,{'content-type':'application/json','cache-control':'no-store'});res.end(JSON.stringify({error:{code:'COORDINATOR_UNAVAILABLE',message:'协调服务暂时不可用'}}));}else res.destroy();}
}
