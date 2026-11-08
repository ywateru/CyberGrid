import {test} from 'node:test';
import assert from 'node:assert/strict';
import {createServer,request as httpRequest} from 'node:http';
import {serveAPI,limited,BodyCapacityError} from '../src/http.js';
test('real adapter uses one 25s budget for partial ingress and remaining handler time',{timeout:32000},async t=>{
 let upstream=0,settled=0;
 const handler=async()=>{upstream++;return new Promise<Response>(()=>{});};
 const server=createServer((req,res)=>void serveAPI(req,res,handler,'https://grid.example').finally(()=>settled++));
 await new Promise<void>(r=>server.listen(0,'127.0.0.1',r));t.after(()=>{server.closeAllConnections();server.close();});
 const address=server.address();assert(address&&typeof address!=='string');
 const run=(finishBody:boolean)=>new Promise<void>((resolve,reject)=>{
  const started=Date.now();const req=httpRequest({host:'127.0.0.1',port:address.port,path:'/api/v1/test',method:'POST',headers:{'content-type':'application/json','transfer-encoding':'chunked'}},res=>{
   res.resume();res.on('end',()=>{try{assert.equal(res.statusCode,503);const elapsed=Date.now()-started;assert(elapsed>=24500&&elapsed<26000,'single ingress deadline elapsed='+elapsed);resolve();}catch(e){reject(e);}});
  });req.on('error',reject);req.write('{');if(finishBody)setTimeout(()=>req.end('}'),1500);
 });
 await Promise.all([run(false),run(true)]);assert.equal(upstream,1);assert.equal(settled,2);
});
test('real partial ingress client disconnect settles without reaching the handler',async t=>{
 let upstream=0;let settle!:()=>void;const settled=new Promise<void>(r=>settle=r);const handler=async()=>{upstream++;return new Response('{}');};
 const server=createServer((req,res)=>void serveAPI(req,res,handler,'https://grid.example').finally(settle));await new Promise<void>(r=>server.listen(0,'127.0.0.1',r));t.after(()=>{server.closeAllConnections();server.close();});const address=server.address();assert(address&&typeof address!=='string');
 const req=httpRequest({host:'127.0.0.1',port:address.port,path:'/api/v1/test',method:'POST',headers:{'transfer-encoding':'chunked'}});req.on('error',()=>{});req.write('{');await new Promise(r=>setTimeout(r,30));req.destroy();await settled;assert.equal(upstream,0);
});

test('bounded response reader aborts a stalled source and rejects overflow',async()=>{
 let canceled=false;const controller=new AbortController();const stream=new ReadableStream<Uint8Array>({cancel(){canceled=true;}});
 const pending=limited(stream,100,controller.signal);controller.abort();await assert.rejects(pending);assert(canceled);assert.equal(stream.locked,false);
 await assert.rejects(limited(new Response('x'.repeat(101)).body,100,new AbortController().signal),BodyCapacityError);
});


test('GET/HEAD framing accepts empty chunked EOF and rejects actual bodies before handler',async t=>{
 let calls=0;
 const server=createServer((req,res)=>void serveAPI(req,res,async request=>{calls++;assert.equal(request.body,null);return new Response('ok');},'https://grid.example'));
 await new Promise<void>(r=>server.listen(0,'127.0.0.1',r));t.after(()=>{server.closeAllConnections();server.close();});
 const address=server.address();assert(address&&typeof address!=='string');
 const send=(method:string,headers:Record<string,string>,body?:string)=>new Promise<number>((resolve,reject)=>{
  const req=httpRequest({host:'127.0.0.1',port:address.port,path:'/framing',method,headers},res=>{res.resume();res.on('end',()=>resolve(res.statusCode!));});
  req.on('error',reject);req.end(body);
 });
 for(const method of ['GET','HEAD']){
  assert.equal(await send(method,{'transfer-encoding':'chunked'}),200);
  assert.equal(await send(method,{'transfer-encoding':'chunked'},'x'),400);
  assert.equal(await send(method,{'content-length':'1'},'x'),400);
  assert.equal(await send(method,{'content-length':'100001'}),413);
 }
 assert.equal(await send('GET',{'transfer-encoding':'chunked'},'x'.repeat(100001)),400);
 assert.equal(calls,2);
});

test('empty chunked GET retains the ingress deadline and disconnect cancellation',{timeout:32000},async t=>{
 let calls=0,settled=0;const waiters:Array<()=>void>=[];
 const server=createServer((req,res)=>void serveAPI(req,res,async()=>{calls++;return new Response('unexpected');},'https://grid.example').finally(()=>{settled++;for(const wake of waiters.splice(0))wake();}));
 await new Promise<void>(r=>server.listen(0,'127.0.0.1',r));t.after(()=>{server.closeAllConnections();server.close();});
 const address=server.address();assert(address&&typeof address!=='string');
 const started=Date.now();
 const stalled=new Promise<void>((resolve,reject)=>{
  const req=httpRequest({host:'127.0.0.1',port:address.port,path:'/framing',method:'GET',headers:{'transfer-encoding':'chunked'}},res=>{
   res.resume();res.on('end',()=>{try{assert.equal(res.statusCode,503);const elapsed=Date.now()-started;assert(elapsed>=24500&&elapsed<27000,'shared ingress deadline elapsed='+elapsed);resolve();}catch(error){reject(error);}});
  });req.on('error',reject);req.flushHeaders();
 });
 const req=httpRequest({host:'127.0.0.1',port:address.port,path:'/framing',method:'GET',headers:{'transfer-encoding':'chunked'}});req.on('error',()=>{});req.flushHeaders();
 await new Promise(r=>setTimeout(r,30));req.destroy();
 if(settled===0)await Promise.race([new Promise<void>(r=>waiters.push(r)),new Promise<never>((_,reject)=>setTimeout(()=>reject(Error('disconnect did not settle')),1000))]);
 assert.equal(settled,1);assert.equal(calls,0);
 await stalled;assert.equal(settled,2);assert.equal(calls,0);
});
