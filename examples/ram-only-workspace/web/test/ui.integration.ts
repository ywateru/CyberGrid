// Actual public UI/official RAM owner + Node/Redis/JWK + controlled SDK wrapper.
// DOM and adapter lifecycle are fixtures, not external Noise or cloud evidence.
import {test} from 'node:test';
import assert from 'node:assert/strict';
import {uiFixture} from './ui-fixture.ts';
const delay=(n=25)=>new Promise<void>(r=>setTimeout(r,n));
function latch(){let release!:()=>void;return {wait:new Promise<void>(r=>release=r),release:()=>release()};}

test('newest UI choice owns the result across old close; cancel and foreground fence pre-dispatch waits',async t=>{
 const f=await uiFixture(t),held=latch(),late=latch();
 const peers:{closed:number;signal:AbortSignal;requests:number}[]=[];
 let holdNext=false;
 const mounted=f.mount({createAdapter:business=>({protocol:'anywork.remote.v1',connect:async(_target,ctx)=>{
  assert.equal((await f.identity.verify(await business.fresh({signal:ctx.signal}))).id,'owner');
  const id=peers.length,p={closed:0,signal:ctx.signal,requests:0};peers.push(p);
  if(holdNext){holdNext=false;await late.wait;}
  let closed!:()=>void;
  return {encrypted:true,identity:{protocol:f.descriptor.protocol,publicKey:f.descriptor.publicKey,installationId:f.descriptor.installationId},limits:f.descriptor.limits,closed:new Promise<void>(r=>closed=r),request:async()=>{p.requests++;return {name:'peer-'+id};},openStream:async()=>{throw Error();},close:async()=>{p.closed++;if(id===0)await held.wait;closed();}};
 }})});
 t.after(()=>{held.release();late.release();return mounted.dispose();});
 await f.login();f.choose();await f.wait(()=>f.nodes.get('status')!.textContent.startsWith('业务已连接'));
 f.choose();await f.wait(()=>peers[0].closed===1);f.choose();await f.wait(()=>peers.length===2&&f.nodes.get('status')!.textContent.startsWith('业务已连接'));
 held.release();await delay(75);assert.equal(peers.length,2,'old pre-dispatch choice never creates another peer');
 f.nodes.get('info')!.onclick!();await f.wait(()=>peers[1].requests===1);assert.equal(peers[1].closed,0);assert.equal(peers[1].signal.aborted,false);
 // A dispatched late result is owned by SDK after cancellation; it cannot commit UI.
 holdNext=true;f.choose();await f.wait(()=>peers.length===3);f.nodes.get('cancel')!.onclick!();late.release();await f.wait(()=>peers[2].closed===1);assert(peers[2].signal.aborted);assert(f.nodes.get('status')!.textContent.startsWith('已取消'));
 f.choose();await f.wait(()=>peers.length===4&&f.nodes.get('status')!.textContent.startsWith('业务已连接'));f.nodes.get('info')!.onclick!();await f.wait(()=>peers[3].requests===1);
 f.setUnavailable(true);f.foreground();await f.wait(()=>f.nodes.get('status')!.textContent.startsWith('目录状态未知'));assert.equal(peers[3].closed,1);assert.equal(f.nodes.get('devices')!.children.length,0);
});

test('stream EOF, split UTF-8, malformed UTF-8, overflow and write failure own cleanup without stale UI',async t=>{
 const f=await uiFixture(t);let mode='valid',cancelled=0,aborted=0,peerClosed=0,writes=0,readController:ReadableStreamDefaultController<Uint8Array>|undefined;
 const mounted=f.mount({createAdapter:business=>({protocol:'anywork.remote.v1',connect:async(_target,ctx)=>{
  await business.fresh({signal:ctx.signal});let done!:()=>void;
  return {encrypted:true,identity:{protocol:f.descriptor.protocol,publicKey:f.descriptor.publicKey,installationId:f.descriptor.installationId},limits:f.descriptor.limits,closed:new Promise<void>(r=>done=r),request:async()=>({name:'healthy'}),close:async()=>{peerClosed++;done();},openStream:async()=>({readable:new ReadableStream<Uint8Array>({start(c){readController=c;if(mode==='overflow')c.enqueue(new Uint8Array(4097));if(mode==='invalid')c.enqueue(new Uint8Array([255]));if(mode==='valid'){const b=new TextEncoder().encode('中文');c.enqueue(b.slice(0,1));c.enqueue(b.slice(1));c.close();}},cancel(){cancelled++;}}),writable:new WritableStream<Uint8Array>({write(){writes++;if(mode==='writefail')throw Error('fixture write');},abort(){aborted++;}})})};
 }})});
 t.after(()=>mounted.dispose());await f.login();
 const connect=async()=>{f.choose();await f.wait(()=>f.nodes.get('status')!.textContent.startsWith('业务已连接'));};
 await connect();f.nodes.get('stream')!.onclick!();await f.wait(()=>aborted===1);assert.equal(f.nodes.get('output')!.textContent,'中文');assert.equal(peerClosed,1,'terminal controlled read is conservatively closed');f.nodes.get('echo')!.onclick!();await delay();assert.equal(writes,0,'normal EOF clears current stream');
 for(const bad of ['overflow','invalid','writefail']){await connect();mode=bad;const before=peerClosed,ab=aborted;f.nodes.get('stream')!.onclick!();if(bad==='writefail'){await delay();f.nodes.get('echo')!.onclick!();}await f.wait(()=>peerClosed===before+1&&(bad==='writefail'||aborted===ab+1));assert(f.nodes.get('status')!.textContent.startsWith('业务已关闭'));const w=writes;f.nodes.get('echo')!.onclick!();await delay();assert.equal(writes,w,'failed stream loses write availability');}
 assert(cancelled>=3);assert(readController);
});

for(const action of ['cancel','foreground-unknown','logout'] as const)test(action+' invalidates choice before its adapter dispatch while old close is held',async t=>{
 const f=await uiFixture(t),held=latch();let created=0,closed=0;
 const mounted=f.mount({createAdapter:business=>({protocol:'anywork.remote.v1',connect:async(_target,ctx)=>{
  await business.fresh({signal:ctx.signal});created++;let done!:()=>void;
  return {encrypted:true,identity:{protocol:f.descriptor.protocol,publicKey:f.descriptor.publicKey,installationId:f.descriptor.installationId},limits:f.descriptor.limits,closed:new Promise<void>(r=>done=r),request:async()=>({}),openStream:async()=>{throw Error();},close:async()=>{closed++;await held.wait;done();}};
 }})});t.after(()=>{held.release();return mounted.dispose();});
 await f.login();f.choose();await f.wait(()=>f.nodes.get('status')!.textContent.startsWith('业务已连接'));f.choose();await f.wait(()=>closed===1);
 if(action==='foreground-unknown'){f.setUnavailable(true);f.foreground();}else f.nodes.get(action)!.onclick!();
 held.release();await delay(100);assert.equal(created,1,'sealed pre-dispatch waiter must never create a peer');
 if(action==='cancel')assert(f.nodes.get('status')!.textContent.startsWith('已取消'));
 if(action==='foreground-unknown')await f.wait(()=>f.nodes.get('status')!.textContent.startsWith('目录状态未知'));
 if(action==='logout')await f.wait(()=>f.nodes.get('status')!.textContent.startsWith('已退出'));
});

test('late read/write errors from retired stream cannot overwrite a newer connected UI',async t=>{
 const f=await uiFixture(t),late=latch();let peers=0,writing=false;
 const mounted=f.mount({createAdapter:business=>({protocol:'anywork.remote.v1',connect:async(_target,ctx)=>{
  await business.fresh({signal:ctx.signal});peers++;let done!:()=>void;
  return {encrypted:true,identity:{protocol:f.descriptor.protocol,publicKey:f.descriptor.publicKey,installationId:f.descriptor.installationId},limits:f.descriptor.limits,closed:new Promise<void>(r=>done=r),request:async()=>({name:'current'}),close:async()=>done(),openStream:async()=>({readable:new ReadableStream<Uint8Array>(),writable:new WritableStream<Uint8Array>({async write(){writing=true;await late.wait;throw Error('late retired write');}})})};
 }})});t.after(()=>{late.release();return mounted.dispose();});await f.login();f.choose();await f.wait(()=>f.nodes.get('status')!.textContent.startsWith('业务已连接'));
 f.nodes.get('stream')!.onclick!();await delay();f.nodes.get('echo')!.onclick!();await f.wait(()=>writing);
 f.choose();f.choose();await f.wait(()=>peers===2&&f.nodes.get('status')!.textContent.startsWith('业务已连接'));late.release();await delay(100);
 assert.equal(peers,2);assert(f.nodes.get('status')!.textContent.startsWith('业务已连接'));assert.equal(f.nodes.get('error')!.textContent,'');
 f.nodes.get('info')!.onclick!();await f.wait(()=>f.nodes.get('output')!.textContent.includes('current'));
});

test('unsettled controlled stream cleanup seals its parent within the local cleanup budget',async t=>{
 const f=await uiFixture(t),settle=latch();let peerClosed=0,opened=0,cancelled=0;
 const mounted=f.mount({createAdapter:business=>({protocol:'anywork.remote.v1',connect:async(_target,ctx)=>{
  await business.fresh({signal:ctx.signal});let done!:()=>void;
  return {encrypted:true,identity:{protocol:f.descriptor.protocol,publicKey:f.descriptor.publicKey,installationId:f.descriptor.installationId},limits:f.descriptor.limits,closed:new Promise<void>(r=>done=r),request:async()=>({}),close:async()=>{peerClosed++;done();},openStream:async()=>{opened++;return {readable:new ReadableStream<Uint8Array>({cancel(){cancelled++;return settle.wait;}}),writable:new WritableStream<Uint8Array>()};}};
 }})});t.after(()=>{settle.release();return mounted.dispose();});await f.login();f.choose();await f.wait(()=>f.nodes.get('status')!.textContent.startsWith('业务已连接'));f.nodes.get('stream')!.onclick!();await f.wait(()=>opened===1);
 const started=performance.now();f.nodes.get('stream')!.onclick!();await f.wait(()=>cancelled===1);await f.wait(()=>peerClosed===1);assert(performance.now()-started<2000);assert(f.nodes.get('status')!.textContent.startsWith('业务已关闭'));assert.equal(opened,1);settle.release();await delay();assert.equal(opened,1,'late stream-close result never opens another stream');
});
