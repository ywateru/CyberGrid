import {test} from 'node:test';
import assert from 'node:assert/strict';
import {createServer,request as httpRequest} from 'node:http';
import {createProxy} from '../src/proxy.js';
import {serveAPI} from '../src/http.js';
const env={CYBERGRID_OWNER_URL:'https://owner.example',CYBERGRID_PUBLIC_ORIGIN:'https://grid.example',CYBERGRID_PROXY_SECRET:'p'.repeat(48)};
test('factory cancellation releases stalled input and output readers and preserves capacity errors',async()=>{
 for(const phase of ['input','output']){
  let canceled=false,fetches=0;const abort=new AbortController();const stream=new ReadableStream<Uint8Array>({cancel(){canceled=true;}});
  const proxy=createProxy(env,(async()=>{fetches++;return new Response(stream);}) as typeof fetch);
  const input=new Request('https://grid.example/api/v1/test',{method:'POST',signal:abort.signal,...(phase==='input'?{body:stream,duplex:'half'}:{body:'{}'})} as RequestInit);
  const pending=proxy(input);await new Promise(r=>setImmediate(r));abort.abort();const response=await pending;
  assert.equal(response.status,503);assert(canceled);assert.equal(fetches,phase==='input'?0:1);assert.equal(stream.locked,false);
 }
 const large=createProxy(env,async()=>new Response(new Uint8Array((2<<20)+1)));
 assert.equal((await large(new Request('https://grid.example/api/v1/test'))).status,503);
 const small=createProxy(env,async()=>{throw Error('must not forward oversized request');});
 assert.equal((await small(new Request('https://grid.example/api/v1/test',{method:'POST',body:'x'.repeat(100001)}))).status,413);
});
test('real adapter uses one 25s budget for partial ingress and remaining upstream time',{timeout:32000},async t=>{
 let upstream=0,settled=0;
 const proxy=createProxy(env,async()=>{upstream++;return new Promise<Response>(()=>{});});
 const server=createServer((req,res)=>void serveAPI(req,res,proxy,true).finally(()=>settled++));
 await new Promise<void>(r=>server.listen(0,'127.0.0.1',r));t.after(()=>{server.closeAllConnections();server.close();});
 const address=server.address();assert(address&&typeof address!=='string');
 const run=(finishBody:boolean)=>new Promise<void>((resolve,reject)=>{
  const started=Date.now();const req=httpRequest({host:'127.0.0.1',port:address.port,path:'/api/v1/test',method:'POST',headers:{'content-type':'application/json','transfer-encoding':'chunked'}},res=>{
   res.resume();res.on('end',()=>{try{assert.equal(res.statusCode,503);const elapsed=Date.now()-started;assert(elapsed>=24500&&elapsed<26000,'single ingress deadline elapsed='+elapsed);resolve();}catch(e){reject(e);}});
  });req.on('error',reject);req.write('{');if(finishBody)setTimeout(()=>req.end('}'),1500);
 });
 await Promise.all([run(false),run(true)]);assert.equal(upstream,1);assert.equal(settled,2);
});
test('real partial ingress client disconnect settles without reaching the owner',async t=>{
 let upstream=0;let settle!:()=>void;const settled=new Promise<void>(r=>settle=r);const proxy=createProxy(env,async()=>{upstream++;return new Response('{}');});
 const server=createServer((req,res)=>void serveAPI(req,res,proxy,true).finally(settle));await new Promise<void>(r=>server.listen(0,'127.0.0.1',r));t.after(()=>{server.closeAllConnections();server.close();});const address=server.address();assert(address&&typeof address!=='string');
 const req=httpRequest({host:'127.0.0.1',port:address.port,path:'/api/v1/test',method:'POST',headers:{'transfer-encoding':'chunked'}});req.on('error',()=>{});req.write('{');await new Promise(r=>setTimeout(r,30));req.destroy();await settled;assert.equal(upstream,0);
});
