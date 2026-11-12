const $=id=>document.getElementById(id);
let session=null,apps=[],refreshTimer=null,busy=false,config=null,spaAccess=null,spaExpiry=0,recovery=null,consoleGeneration=0;
let selectionRevision=0,deviceRevision=0,appsRevision=0,catalogReady=false,deviceAbort=null,appsAbort=null;
const fixedErrors=Object.freeze({CANCELLED:'操作已取消。',NETWORK:'暂时无法读取目录，请稍后重试。',INVALID_RESPONSE:'目录响应无效，请稍后重试。',DIRECTORY_TOO_LARGE:'目录超过响应容量，状态未知。',FORBIDDEN:'当前无权读取目录，状态未知。',SESSION_UNAVAILABLE:'登录会话不可用，请重新登录。',REQUEST_FAILED:'请求未完成，请稍后重试。'});
function failure(code){const e=new Error(fixedErrors[code]||fixedErrors.REQUEST_FAILED);e.code=Object.hasOwn(fixedErrors,code)?code:'REQUEST_FAILED';return e;}
function safeMessage(error){return fixedErrors[error?.code]||fixedErrors.REQUEST_FAILED;}
function node(tag,cls,text){const n=document.createElement(tag);if(cls)n.className=cls;if(text!==undefined)n.textContent=text;return n;}
function message(text,error=false){$('message').textContent=text;$('message').classList.toggle('error',error);}
function checkCurrent(generation,guard){if(generation!==consoleGeneration||guard&&!guard())throw failure('CANCELLED');}
async function readJSON(response){
 const reader=response.body?.getReader();if(!reader)throw failure('INVALID_RESPONSE');
 const chunks=[];let size=0;
 try{for(;;){const {done,value}=await reader.read();if(done)break;size+=value.byteLength;if(size>2*1024*1024)throw failure('DIRECTORY_TOO_LARGE');chunks.push(value);}}
 catch(error){await reader.cancel().catch(()=>{});throw Object.hasOwn(fixedErrors,error?.code)?error:failure('NETWORK');}
 finally{reader.releaseLock();}
 const bytes=new Uint8Array(size);let offset=0;for(const chunk of chunks){bytes.set(chunk,offset);offset+=chunk.byteLength;}
 try{return JSON.parse(new TextDecoder('utf-8',{fatal:true}).decode(bytes));}catch{throw failure('INVALID_RESPONSE');}
}
async function api(path,method='GET',body,canRecover=true,guard,signal){
 const generation=consoleGeneration;checkCurrent(generation,guard);
 const headers={};if(session){headers.authorization='Bearer '+session.sessionKey;headers['X-CyberGrid-Epoch']=session.coordinatorEpoch;}if(body!==undefined)headers['content-type']='application/json';
 let r,data;
 try{r=await fetch('/api/v1'+path,{method,headers,body:body===undefined?undefined:JSON.stringify(body),cache:'no-store',signal:signal?AbortSignal.any([signal,AbortSignal.timeout(20000)]):AbortSignal.timeout(20000)});checkCurrent(generation,guard);data=await readJSON(r);checkCurrent(generation,guard);}
 catch(error){checkCurrent(generation,guard);if(signal?.aborted)throw failure('CANCELLED');throw Object.hasOwn(fixedErrors,error?.code)?error:failure('NETWORK');}
 const changed=method==='GET'&&!path.includes('/auth/')&&session&&r.headers.get('x-cybergrid-epoch')&&r.headers.get('x-cybergrid-epoch')!==session.coordinatorEpoch;
 if(canRecover&&session&&method==='GET'&&(changed||!r.ok&&data?.error?.code==='COORDINATOR_RESET')){await recoverConsole();checkCurrent(generation,guard);return api(path,method,body,false,guard,signal);}
 if(!r.ok){
  if(r.status===401&&session){clearSession();throw failure('SESSION_UNAVAILABLE');}
  if(r.status===413||data?.error?.code==='DIRECTORY_TOO_LARGE')throw failure('DIRECTORY_TOO_LARGE');
  if(r.status===403||data?.error?.code==='APP_UNAVAILABLE')throw failure('FORBIDDEN');
  throw failure('REQUEST_FAILED');
 }
 return data;
}
function setBusy(value){busy=value;$('login').disabled=value;$('app-select').disabled=value||!session||!apps.length;$('copy-app').disabled=value||!session||!apps.length;$('refresh').disabled=value||!session;}
function empty(title,description){const wrap=node('div','empty-state');wrap.append(node('h3','',title),node('p','',description));$('devices').replaceChildren(wrap);}
function unknown(){ $('device-subtitle').textContent='状态未知';$('refreshed').textContent='';empty('设备状态未知','正在读取或暂时无法确认当前产品目录；历史在线状态不作为当前结论。');}
function invalidateReads(){selectionRevision++;deviceRevision++;appsRevision++;deviceAbort?.abort();appsAbort?.abort();deviceAbort=null;appsAbort=null;}
function clearSession(){consoleGeneration++;catalogReady=false;invalidateReads();recovery=null;session=null;spaAccess=null;spaExpiry=0;clearInterval(refreshTimer);refreshTimer=null;$('identity').textContent='未登录';$('credentials').hidden=false;$('logout').hidden=true;$('refresh').disabled=true;$('application-panel').hidden=true;apps=[];$('app-select').replaceChildren();$('app-id').textContent='';unknown();}
const object=value=>value!==null&&typeof value==='object'&&!Array.isArray(value);
const text=(value,max)=>typeof value==='string'&&value.length>0&&value.length<=max&&!/[\u0000-\u001f\u007f]/.test(value);
function parseApps(data){
 if(!object(data)||!['static','logto'].includes(data.source)||!Array.isArray(data.apps)||data.apps.length>10000)throw failure('INVALID_RESPONSE');
 const ids=new Set();for(const a of data.apps){if(!object(a)||!text(a.id,21)||!text(a.name,256)||a.type!=='Native'||a.isDeviceFlow!==false||ids.has(a.id))throw failure('INVALID_RESPONSE');ids.add(a.id);}
 return data.apps.map(({id,name})=>({id,name}));
}
function parseDevices(data){
 if(!object(data)||!Array.isArray(data.devices)||data.devices.length>10000)throw failure('INVALID_RESPONSE');
 const ids=new Set();return data.devices.map(d=>{
  if(!object(d)||!text(d.id,96)||!text(d.name,256)||typeof d.online!=='boolean'||typeof d.lastSeen!=='string'||!Number.isFinite(Date.parse(d.lastSeen))||!Array.isArray(d.ports)||d.ports.length>1024||ids.has(d.id))throw failure('INVALID_RESPONSE');ids.add(d.id);
  const ports=d.ports.map(p=>{if(!object(p)||!text(p.type,64)||p.transport!=='tcp'||!Number.isSafeInteger(p.port)||p.port<1||p.port>65535)throw failure('INVALID_RESPONSE');return {type:p.type,transport:p.transport,port:p.port};});
  return {id:d.id,name:d.name,online:d.online,lastSeen:d.lastSeen,ports};
 });
}
async function loadApps(){
 const generation=consoleGeneration,selection=selectionRevision,revision=++appsRevision;
 // A new catalog read revokes every older device snapshot, even if fetch ignores abort.
 catalogReady=false;deviceRevision++;deviceAbort?.abort();deviceAbort=null;
 appsAbort?.abort();const controller=appsAbort=new AbortController();
 const current=()=>session&&selection===selectionRevision&&revision===appsRevision;
 unknown();
 try{
  const next=parseApps(await api('/apps','GET',undefined,true,current,controller.signal));checkCurrent(generation,current);
  const selected=$('app-select').value;apps=next;
  $('app-select').replaceChildren(...apps.map(a=>{const o=node('option','',a.name);o.value=a.id;return o;}));
  $('app-select').value=apps.some(a=>a.id===selected)?selected:apps[0]?.id||'';
  if($('app-select').value!==selected){selectionRevision++;deviceRevision++;deviceAbort?.abort();}
  catalogReady=true;$('application-panel').hidden=false;showAppID();
  if(!apps.length)message('暂无可用产品。请检查静态产品策略或可选的动态 Logto 目录。',true);
 }catch(error){if(generation===consoleGeneration&&current()){unknown();message(safeMessage(error),true);}throw error;}
 finally{if(appsAbort===controller)appsAbort=null;}
}
async function refreshDevices(){
 if(!session)return;
 const generation=consoleGeneration,selection=selectionRevision,catalog=appsRevision,selected=$('app-select').value,revision=++deviceRevision;
 deviceAbort?.abort();const controller=deviceAbort=new AbortController();
 const current=()=>session&&catalogReady&&catalog===appsRevision&&apps.some(a=>a.id===selected)&&selection===selectionRevision&&selected===$('app-select').value&&revision===deviceRevision;
 unknown();
 if(!catalogReady||!apps.some(a=>a.id===selected)){if(deviceAbort===controller)deviceAbort=null;return;}
 try{
  const devices=parseDevices(await api('/console/devices?appId='+encodeURIComponent(selected),'GET',undefined,true,current,controller.signal));checkCurrent(generation,current);
  // Validate and prepare the complete snapshot before committing any online cards.
  const cards=devices.map(d=>{const wrap=node('article','device'),title=node('div','device-title');title.append(node('h3','',d.name),node('span',d.online?'badge':'badge muted',d.online?'在线':'离线'));wrap.append(title,node('p','device-id',`设备 ${d.id} · ${new Date(d.lastSeen).toLocaleString('zh-CN')}`));const ports=node('div','ports');if(!d.ports.length)ports.append(node('span','no-ports','此终端尚未发布服务端口'));for(const p of d.ports)ports.append(node('span','port',`${p.type} / ${p.transport} / ${p.port}`));wrap.append(ports);return wrap;});
  checkCurrent(generation,current);$('devices').replaceChildren(...cards);if(!devices.length)empty('还没有其他终端','在各端独立登录同账号、同产品后查看。');
  $('device-subtitle').textContent=`${devices.filter(d=>d.online).length} 台在线，共 ${devices.length} 台终端`;$('refreshed').textContent='更新于 '+new Date().toLocaleTimeString('zh-CN');
 }catch(error){if(generation===consoleGeneration&&current()){unknown();message(safeMessage(error),true);}}
 finally{if(deviceAbort===controller)deviceAbort=null;}
}
const b64=bytes=>btoa(String.fromCharCode(...bytes)).replace(/=/g,'').replace(/\+/g,'-').replace(/\//g,'_');
const random=()=>b64(crypto.getRandomValues(new Uint8Array(32)));
function decode(v){return Uint8Array.from(atob(v.replace(/-/g,'+').replace(/_/g,'/')),c=>c.charCodeAt(0));}
async function verifyID(token,nonce){const parts=token?.split('.');if(parts?.length!==3)throw new Error('身份响应不完整，请重新登录。');const h=JSON.parse(new TextDecoder().decode(decode(parts[0]))),p=JSON.parse(new TextDecoder().decode(decode(parts[1])));if(h.alg!=='ES384'||typeof h.kid!=='string'||p.iss!==config.issuer||p.aud!==config.clientId||p.nonce!==nonce||typeof p.sub!=='string'||typeof p.exp!=='number'||typeof p.iat!=='number'||p.exp<=Date.now()/1000||p.iat>Date.now()/1000+30)throw new Error('身份校验失败，请重新登录。');const response=await fetch(config.jwksUri,{cache:'no-store',signal:AbortSignal.timeout(15000)});if(!response.ok)throw new Error('身份签名服务暂不可用。');const {keys}=await response.json();const jwk=keys.find(k=>k.kid===h.kid&&k.kty==='EC'&&k.crv==='P-384'&&(!k.use||k.use==='sig'));if(!jwk)throw new Error('身份签名无法识别。');const key=await crypto.subtle.importKey('jwk',jwk,{name:'ECDSA',namedCurve:'P-384'},false,['verify']);if(!await crypto.subtle.verify({name:'ECDSA',hash:'SHA-384'},key,decode(parts[2]),new TextEncoder().encode(parts[0]+'.'+parts[1])))throw new Error('身份签名无效。');return p;}
async function tokenRequest(body){const r=await fetch(config.tokenEndpoint,{method:'POST',headers:{'content-type':'application/x-www-form-urlencoded'},body:new URLSearchParams(body),signal:AbortSignal.timeout(20000)});if(!r.ok)throw new Error('Logto 授权未完成，请重新登录。');return r.json();}
async function beginLogin(){const generation=consoleGeneration;setBusy(true);try{const transient={state:random(),nonce:random(),verifier:random(),createdAt:Date.now()};const challenge=b64(new Uint8Array(await crypto.subtle.digest('SHA-256',new TextEncoder().encode(transient.verifier))));checkCurrent(generation);sessionStorage.setItem('cybergrid.pkce',JSON.stringify(transient));const url=new URL(config.authorizationEndpoint);url.search=new URLSearchParams({client_id:config.clientId,redirect_uri:config.redirectUri,response_type:'code',scope:config.scope,resource:config.resource,state:transient.state,nonce:transient.nonce,code_challenge:challenge,code_challenge_method:'S256'});location.assign(url);}catch{message('无法启动登录，请允许当前页面使用临时会话存储后重试。',true);setBusy(false);}}
async function callback(query){
 const generation=consoleGeneration;
 const current=()=>{if(generation!==consoleGeneration)throw new Error('Login operation canceled.');};
 const raw=sessionStorage.getItem('cybergrid.pkce');sessionStorage.removeItem('cybergrid.pkce');history.replaceState(null,'','/console/');
 if(query.has('error'))throw new Error('登录已取消或授权未完成，请重新登录。');
 const t=raw?JSON.parse(raw):null;
 if(!t||t.state!==query.get('state')||typeof t.createdAt!=='number'||Date.now()-t.createdAt>600000||t.createdAt>Date.now()+30000||!query.get('code'))throw new Error('登录校验已过期，请重新登录。');
 let tokens;
 try{
  tokens=await tokenRequest({client_id:config.clientId,grant_type:'authorization_code',code:query.get('code'),code_verifier:t.verifier,redirect_uri:config.redirectUri,resource:config.resource});current();
  const identity=await verifyID(tokens.id_token,t.nonce);current();
  if(tokens.access_token?.split('.').length!==3){
   if(!tokens.refresh_token)throw new Error('未取得应用访问授权，请重新登录。');
   tokens=await tokenRequest({client_id:config.clientId,grant_type:'refresh_token',refresh_token:tokens.refresh_token,resource:config.resource,scope:config.scope});current();
  }
  const next=await api('/console/auth/exchange','POST',{accessToken:tokens.access_token,coordinationProtocol:'memory-v1',coordinatorEpoch:config.coordination?.epoch,requestId:crypto.randomUUID()},false);current();
  if(next.purpose!=='console'||next.deviceId||next.appId||next.user?.id!==identity.sub||next.coordinatorEpoch!==config.coordination?.epoch)throw new Error('控制台会话类型无效。');
  session=next;spaAccess=tokens.access_token;spaExpiry=Date.now()+Math.max(0,Number(tokens.expires_in)||0)*1000;
 }catch(error){if(generation===consoleGeneration)clearSession();throw error;}finally{tokens=null;}
 $('identity').textContent=session.user.email||'已登录';$('credentials').hidden=true;$('logout').hidden=false;message('已登录。终端状态每 20 秒更新。');
 await loadApps();current();await refreshDevices();current();refreshTimer=setInterval(refreshDevices,20000);
}
async function recoverConsole(){if(recovery)return recovery;unknown();const generation=consoleGeneration,subject=session?.user.id;const pending=(async()=>{if(!spaAccess||spaExpiry<=Date.now()+5000){clearSession();throw new Error('协调会话已重置，请重新登录。');}const fresh=await api('/console/auth/config','GET',undefined,false);for(const field of ['issuer','clientId','resource','scope','deviceAuthorizationEndpoint','authorizationEndpoint','tokenEndpoint','revocationEndpoint','jwksUri','redirectUri'])if(fresh[field]!==config[field]){clearSession();throw new Error('登录配置已变化，请重新登录。');}if(fresh.coordination?.protocol!=='memory-v1')throw new Error('协调协议不可用。');config=fresh;const next=await api('/console/auth/exchange','POST',{accessToken:spaAccess,coordinationProtocol:'memory-v1',coordinatorEpoch:config.coordination.epoch,requestId:crypto.randomUUID()},false);if(generation!==consoleGeneration)throw new Error('登录操作已取消。');if(next.purpose!=='console'||next.deviceId||next.appId||next.user.id!==subject||next.coordinatorEpoch!==config.coordination.epoch){clearSession();throw new Error('控制台恢复身份校验失败。');}session=next;message('协调服务已恢复，正在更新终端状态。');})().catch(e=>{if(generation===consoleGeneration)clearSession();throw e;}).finally(()=>{if(recovery===pending)recovery=null;});recovery=pending;return pending;}
$('login').addEventListener('click',beginLogin);
async function logoutConsole(){
 if(busy)return;
 const previous=session;clearSession();const generation=consoleGeneration;setBusy(true);
 try{
  if(previous){const response=await fetch('/api/v1/session',{method:'DELETE',headers:{authorization:'Bearer '+previous.sessionKey,'X-CyberGrid-Epoch':previous.coordinatorEpoch},cache:'no-store',signal:AbortSignal.timeout(20000)});if(!response.ok)throw new Error('Remote logout not confirmed');}
  if(generation===consoleGeneration)message('已撤销本控制台会话。其他终端会话不受影响。');
 }catch{if(generation===consoleGeneration)message('本机已退出，中央注销未确认；中央会话将在到期后失效。',true);}
 finally{if(generation===consoleGeneration)setBusy(false);}
}
$('logout').addEventListener('click',logoutConsole);
function showAppID(){$('app-id').textContent=$('app-select').value;}
$('app-select').addEventListener('change',async()=>{invalidateReads();unknown();showAppID();await refreshDevices();});
$('copy-app').addEventListener('click',async()=>{try{await navigator.clipboard.writeText($('app-select').value);message('已复制产品 ID；OAuth client ID 请按部署策略分别配置。');}catch{message('请选中应用 ID 手动复制。',true);}});
$('refresh').addEventListener('click',async()=>{const generation=consoleGeneration;setBusy(true);try{await loadApps();checkCurrent(generation);await refreshDevices();}catch(e){if(generation===consoleGeneration&&e?.code!=='CANCELLED')message(safeMessage(e),true);}finally{if(generation===consoleGeneration)setBusy(false);}});
window.addEventListener('pagehide',()=>{clearInterval(refreshTimer);refreshTimer=null;consoleGeneration++;catalogReady=false;invalidateReads();session=null;spaAccess=null;spaExpiry=0;});
(async()=>{const query=new URLSearchParams(location.search),requestedPath=location.pathname;setBusy(true);try{config=await api('/console/auth/config');const redirect=new URL(config.redirectUri);const isCallback=redirect.origin===location.origin&&requestedPath===redirect.pathname;if(isCallback){history.replaceState(null,'','/console/');await callback(query);}else message('通过 Logto 登录或注册，随后查看应用和你的设备。');}catch(e){if(query.has('code')||query.has('error'))history.replaceState(null,'','/console/');message(safeMessage(e),true);}finally{setBusy(false);}})();
