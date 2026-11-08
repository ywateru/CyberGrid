import type {IncomingMessage,ServerResponse} from 'node:http';
import {createProxy} from '../src/proxy.js';
import {serveAPI} from '../src/http.js';
export const config={api:{bodyParser:false}};
let proxy:ReturnType<typeof createProxy>|undefined;
export default async function handler(req:IncomingMessage,res:ServerResponse){try{proxy??=createProxy();await serveAPI(req,res,proxy,true);}catch{res.writeHead(503,{'content-type':'application/json','cache-control':'no-store'});res.end(JSON.stringify({error:{code:'SERVICE_NOT_CONFIGURED',message:'无状态代理尚未配置'}}));}}
