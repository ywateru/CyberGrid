import {CyberGridClient,type DirectoryDevice,type DirectoryWatch,type SecureBridge,type SecureBridgeStream} from '../../../../sdk-js/src/index.js';
import {SpaAuthorization,deliverCallback} from './authorization.ts';
import type {WebConfig,WebDependencies,SampleRequest,SampleResponse,SampleStreamInput} from './types.ts';
export type {WebConfig,WebDependencies,BusinessAuthorization,SampleRequest,SampleResponse,SampleStreamInput} from './types.ts';
export type {TrustedBridgeAdapter,AuthenticatedBridgePeer,BridgeTarget} from '../../../../sdk-js/src/index.js';
/** Only controlled SDK operations escape; the external transport stays inside its adapter. */
export function mountRemoteWorkspace(element:HTMLElement,config:Readonly<WebConfig>,dependencies:WebDependencies={}) {
 const pageWindow=window,pageDocument=document;
 let disposal:Promise<void>|undefined;
 let generation=0,owner:SpaAuthorization|undefined,client:CyberGridClient|undefined,watch:DirectoryWatch|undefined;
 let bridge:SecureBridge<SampleRequest,SampleResponse,SampleStreamInput>|undefined;
 let stream:SecureBridgeStream|undefined,disposed=false,devices:readonly DirectoryDevice[]=[];
 let attemptRevision=0,streamRevision=0;
 type Attempt={revision:number;account:number;abort:AbortController;pending:boolean};
 let attempt:Attempt|undefined;
 // Each dispatched close keeps an owner even when its UI waiter is superseded.
 const cleanup=new Set<Promise<unknown>>();
 function own<T>(work:Promise<T>):Promise<T>{cleanup.add(work);void work.then(()=>cleanup.delete(work),()=>cleanup.delete(work));return work;}
 function invalidateAttempt(){++attemptRevision;attempt?.abort.abort();attempt=undefined;}
 function live(a:Attempt){return !disposed&&a.account===generation&&a.revision===attemptRevision&&attempt===a&&!a.abort.signal.aborted;}
 function closeStream(s:SecureBridgeStream,parent:SecureBridge<SampleRequest,SampleResponse,SampleStreamInput>){
  const work=own(s.close());let timer:ReturnType<typeof setTimeout>;
  // A non-cooperative stream cannot keep its parent usable indefinitely. The
  // actual close remains owned; the SDK bounds the parent cleanup wait.
  const fallback=()=>{if(bridge===parent){invalidateAttempt();++streamRevision;bridge=undefined;stream=undefined;status('业务已关闭：回声流清理尚未确认。');}return own(parent.close());};
  timer=setTimeout(()=>{void fallback().catch(()=>{});},1000);
  void work.then(()=>clearTimeout(timer),()=>{clearTimeout(timer);void fallback().catch(()=>{});});
  return work;
 }
 const trustedKeys=new Map<string,string>(); // RAM only; never turn installation association into trust.
 element.innerHTML=`<header><h1>CyberGrid Workspace</h1><p>远程合成工作区 · RAM-only</p><button data-login>网页登录</button><button data-logout>退出本机授权</button></header><section><h2>我的设备</h2><p data-status role="status">请先登录。刷新页面需重新授权。</p><p data-adapter></p><div data-devices></div><button data-cancel>取消连接</button></section><section><h2>合成工作区</h2><button data-info>读取信息</button><button data-list>读取记录</button><button data-create>明确添加测试记录</button><button data-stream>打开 UTF-8 回声流</button><button data-echo>发送合成回声</button><pre data-output></pre></section><p data-error role="alert"></p><footer>目录发现、发布确认与业务连接分别显示。未知写入结果不会自动重试。</footer>`;
 const node=(key:string)=>element.querySelector<HTMLElement>('[data-'+key+']')!;
 const status=(text:string)=>{node('status').textContent=text;};
 const issue=(text:string)=>{node('error').textContent=text;};
 node('adapter').textContent=dependencies.createAdapter?'已注入受信业务 adapter。':'未安装业务 adapter：可查看设备，远程业务不可用。';
 function closeBusiness():Promise<unknown>{invalidateAttempt();++streamRevision;const old=bridge,s=stream;bridge=undefined;stream=undefined;if(s&&old)void closeStream(s,old).catch(()=>{});return old?own(old.close()):Promise.resolve();}
 async function end(){++generation;watch?.stop();watch=undefined;await closeBusiness();const c=client,o=owner;devices=[];renderDevices();const cleaned=c?.disconnect();await o?.end();await cleaned;if(client===c)client=undefined;if(owner===o)owner=undefined;status('已退出本机授权。云端 SSO 状态未作全局退出。');}
 async function unknown(){devices=[];renderDevices();status('目录状态未知；请检查服务后显式重新连接。');await closeBusiness();}
 function renderDevices(){const list=node('devices');list.replaceChildren();for(const device of devices){const item=pageDocument.createElement('article'),label=pageDocument.createElement('p');label.textContent=`${device.name} · ${device.online?'在线':'离线'} · ${device.bridges.length} 个业务入口`;item.append(label);
  for(const descriptor of device.bridges){const b=pageDocument.createElement('button');b.textContent='确认身份并连接';b.disabled=!device.online||!dependencies.createAdapter;b.onclick=()=>void choose(device,descriptor.id);item.append(b);}list.append(item);}}
 async function choose(device:DirectoryDevice,bridgeId:string){
  const current=client,o=owner,g=generation;if(!current||!o||!dependencies.createAdapter)return;
  const closing=closeBusiness();const a:Attempt={revision:attemptRevision,account:g,abort:new AbortController(),pending:true};attempt=a;
  await closing;if(!live(a))return;
  const descriptor=device.bridges.find(v=>v.id===bridgeId);if(!descriptor)return;
  const slot=config.productId+'|'+device.id+'|'+bridgeId,old=trustedKeys.get(slot);
  const identity=JSON.stringify([descriptor.installationId,descriptor.publicKey]);
  if(old!==identity&&!pageWindow.confirm(`${old?'此设备握手公钥已变化。':'首次连接此设备。'}安装标识仅用于展示，不能代替公钥确认。\n${descriptor.publicKey}\n确认本次身份后继续？`))return;
  if(!live(a))return;const abort=a.abort;issue('');status('连接中：总预算 60 秒，可随时取消。');
  try{const delegate=dependencies.createAdapter(o.business());
   const adapter:typeof delegate={protocol:delegate.protocol,connect:(target,context)=>{
    if(JSON.stringify([target.descriptor.installationId,target.descriptor.publicKey])!==identity)throw Error('identity confirmation changed');
    return delegate.connect(target,context);
   }};
   const result=await current.connectBridge(config.productId,device.id,bridgeId,adapter,{signal:abort.signal,timeoutMs:60000});
   if(!live(a)){await own(result.close());return;}
   a.pending=false;bridge=result;trustedKeys.set(slot,identity);status('业务已连接：已完成受信 adapter 的加密身份握手。');
  }catch{if(live(a)){a.pending=false;status('业务未连接。请检查目录与授权后显式重试。');issue('连接未完成；取消不会交付迟到的连接。');}}
 }
 function beginLogin(){
  if(disposed||owner){issue('已有 RAM 授权或正在登录，请先退出。');return;}
  const g=++generation;const o=new SpaAuthorization(config,g);owner=o;
  // Keep this call synchronous with the user click, before any async discovery.
  const login=o.login();status('正在网页登录。');
  void(async()=>{try{await login;if(disposed||g!==generation)return;
   const c=new CyberGridClient({baseUrl:config.server,issuer:config.issuer,clientId:config.clientId,resource:config.resource,getAccessToken:r=>o.coordination(r)});client=c;await c.connect();if(g!==generation)return;
   status('已登录；目录发布能力不代表业务已连接。');watch=c.watchDirectory(config.productId,event=>{
    if(g!==generation||disposed)return;if(event.type==='unknown'){void unknown();return;}
    devices=event.snapshot.devices;renderDevices();if(!bridge)status('设备目录已确认；请选择设备并确认握手身份。');
   },{intervalMs:2000});
  }catch{if(g===generation){status('登录未完成；请退出本机授权后重试。');issue('浏览器回调、固定配置或授权验证未完成。');}}})();
 }
 async function request(input:SampleRequest){const current=bridge;if(!current){issue('请先连接设备。');return;}try{const result=await current.request(input);if(current===bridge)node('output').textContent=JSON.stringify(result,null,2).slice(0,8192);}catch{if(current===bridge)issue(input.kind==='create-session'?'写入结果未知；不会自动重试。请先读取当前记录。':'读取未完成。请检查连接后显式重试。');}}
 node('login').onclick=()=>{try{beginLogin();}catch{issue('无法创建授权窗口或配置无效。');}};
 node('logout').onclick=()=>void end().catch(()=>issue('本机授权仍在安全收尾，请等待。'));
 node('cancel').onclick=()=>{if(attempt?.pending){invalidateAttempt();status('已取消连接等待；底层迟到结果由 SDK 继续关闭。');}};
 node('info').onclick=()=>void request({kind:'info'});node('list').onclick=()=>void request({kind:'list-sessions'});
 node('create').onclick=()=>void request({kind:'create-session',value:'synthetic-'+crypto.randomUUID()});
 function failStream(s:SecureBridgeStream,parent:SecureBridge<SampleRequest,SampleResponse,SampleStreamInput>,seq:number,message:string){
  const current=stream===s&&bridge===parent&&streamRevision===seq;
  if(current){stream=undefined;void closeBusiness().catch(()=>{});status('业务已关闭：回声流未正常完成。');issue(message);}
  void closeStream(s,parent).catch(()=>{});
 }
 node('stream').onclick=()=>{const parent=bridge;if(!parent)return;const seq=++streamRevision,g=generation,prior=stream;stream=undefined;
  void(async()=>{try{if(prior)await closeStream(prior,parent);if(parent!==bridge||seq!==streamRevision||g!==generation)return;
   const s=await parent.openStream({kind:'echo',sessionId:'synthetic'});
   if(parent!==bridge||seq!==streamRevision||g!==generation){void closeStream(s,parent).catch(()=>{});return;}
   stream=s;const decoder=new TextDecoder('utf-8',{fatal:true});
   void(async()=>{try{for(;;){const bytes=await s.read();if(stream!==s||parent!==bridge||seq!==streamRevision)return;
    if(bytes===null){const tail=decoder.decode();if(tail)node('output').textContent=tail;stream=undefined;void closeStream(s,parent).catch(()=>{});break;}
    if(bytes.length>4096)throw Error();node('output').textContent=decoder.decode(bytes,{stream:true});
   }}catch{failStream(s,parent,seq,'回声流已结束或返回超限内容。');}})();
  }catch{if(parent===bridge&&seq===streamRevision&&g===generation)issue('流未建立。');}})();
 };
 node('echo').onclick=()=>{const s=stream,parent=bridge,seq=streamRevision;if(s&&parent)void s.write(new TextEncoder().encode('CyberGrid 合成回声')).catch(()=>failStream(s,parent,seq,'流写入未完成；不会重放。'));};
 const foreground=()=>{if(pageDocument.visibilityState==='hidden')return;void(async()=>{const closed=closeBusiness(),revision=attemptRevision,g=generation;await closed;const c=client;if(!c||g!==generation||revision!==attemptRevision)return;status('恢复前台：正在重新确认目录和授权。');try{await c.directory(config.productId);if(g===generation&&revision===attemptRevision)status('目录已重新确认；请显式重连业务。');}catch{if(g===generation&&revision===attemptRevision)await unknown();}})();};
 pageWindow.addEventListener('pageshow',foreground);pageDocument.addEventListener('visibilitychange',foreground);
 return Object.freeze({dispose:()=>{if(disposal)return disposal;disposed=true;pageWindow.removeEventListener('pageshow',foreground);pageDocument.removeEventListener('visibilitychange',foreground);disposal=end().finally(()=>element.replaceChildren());return disposal;}});
}
export {deliverCallback};
