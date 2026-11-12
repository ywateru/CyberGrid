import {test} from 'node:test';import assert from 'node:assert/strict';import {readFile}from'node:fs/promises';import vm from'node:vm';
async function fixture(){const source=await readFile(new URL('../public/console/console.js',import.meta.url),'utf8');const elements=new Map<string,any>();const events=new Map<string,()=>void>();let intervals=0;const element=()=>({disabled:false,hidden:false,value:'',textContent:'',children:[] as any[],listeners:{} as Record<string,Function>,classList:{toggle(){}},addEventListener(name:string,fn:Function){this.listeners[name]=fn;},replaceChildren(...children:any[]){this.children=children;},append(...children:any[]){this.children.push(...children);}});let fetcher:typeof fetch=async()=>new Response('{}');const context=vm.createContext({document:{getElementById:(id:string)=>{if(!elements.has(id))elements.set(id,element());return elements.get(id);},createElement:element},window:{addEventListener:(name:string,fn:()=>void)=>events.set(name,fn)},location:{search:'',pathname:'/console/'},history:{replaceState(){}},sessionStorage:{getItem(){return null;},removeItem(){},setItem(){throw Error('tokens must not persist');}},setInterval(){intervals++;return intervals;},clearInterval(){intervals=0;},AbortSignal,AbortController,URLSearchParams,URL,crypto:globalThis.crypto,TextEncoder,TextDecoder,Uint8Array,fetch:(...args:Parameters<typeof fetch>)=>fetcher(...args)});vm.runInContext(source,context);await new Promise(r=>setImmediate(r));return{run:(s:string)=>vm.runInContext(s,context),fetch:(f:typeof fetch)=>fetcher=f,event:(name:string)=>events.get(name)?.(),intervals:()=>intervals};}
test('console RAM recovery binds OAuth configuration and rejects logout-racing responses',async()=>{const f=await fixture();f.run("config={issuer:'https://id.example/oidc',clientId:'SPA',scope:'openid connect',tokenEndpoint:'https://id.example/token',coordination:{protocol:'memory-v1',epoch:'old'}};session={purpose:'console',user:{id:'subject'},sessionKey:'private-old',coordinatorEpoch:'old'};spaAccess='private-access';spaExpiry=Date.now()+60000");let exchanges=0;f.fetch(async(input)=>{if(String(input).endsWith('/console/auth/config'))return new Response(JSON.stringify({issuer:'https://id.example/oidc',clientId:'SPA',scope:'openid connect',tokenEndpoint:'https://id.example/token',coordination:{protocol:'memory-v1',epoch:'new'}}));exchanges++;return new Response(JSON.stringify({purpose:'console',user:{id:'subject'},sessionKey:'private-new',coordinatorEpoch:'new'}));});await f.run('recoverConsole()');assert.equal(f.run('session.coordinatorEpoch'),'new');assert.equal(exchanges,1);let release!:(r:Response)=>void;f.fetch(()=>new Promise(r=>release=r));const pending=f.run("api('/apps')");f.run('clearSession()');release(new Response('{"apps":[]}'));await assert.rejects(pending,(e:any)=>e.code==='CANCELLED');assert.equal(f.run('session'),null);assert.equal(f.run('spaAccess'),null);});
test('console refuses scope/endpoint mutation before resending resource token',async()=>{const f=await fixture();f.run("config={issuer:'https://id.example/oidc',clientId:'SPA',scope:'openid connect',tokenEndpoint:'https://id.example/token',coordination:{protocol:'memory-v1',epoch:'old'}};session={purpose:'console',user:{id:'subject'},sessionKey:'private-old',coordinatorEpoch:'old'};spaAccess='private-access';spaExpiry=Date.now()+60000");let count=0;f.fetch(async()=>{count++;return new Response(JSON.stringify({issuer:'https://id.example/oidc',clientId:'SPA',scope:'openid connect extra',tokenEndpoint:'https://id.example/token',coordination:{protocol:'memory-v1',epoch:'new'}}));});await assert.rejects(f.run('recoverConsole()'));assert.equal(count,1);assert.equal(f.run('session'),null);assert.equal(f.run('spaAccess'),null);});

for(const phase of ['token','verify','exchange','apps'])test(`initial callback cannot outlive pagehide at ${phase}`,async()=>{
 const f=await fixture();f.run("config={clientId:'SPA',tokenEndpoint:'https://id.example/token',coordination:{epoch:'epoch'}};sessionStorage.getItem=()=>JSON.stringify({state:'state',nonce:'nonce',verifier:'verifier',createdAt:Date.now()});verifyID=async()=>({sub:'subject'})");
 let release!:(value?:any)=>void;let exchanges=0;const next={purpose:'console',user:{id:'subject'},sessionKey:'test-session-key',coordinatorEpoch:'epoch'};
 if(phase==='verify')f.run('verifyID=()=>new Promise(resolve=>globalThis.releaseVerify=resolve)');
 f.fetch(async input=>{const url=String(input);if(url==='https://id.example/token'){if(phase==='token')return new Promise(r=>release=r);return new Response(JSON.stringify({access_token:'test.resource.jwt',id_token:'test',expires_in:600}));}if(url.endsWith('/console/auth/exchange')){exchanges++;if(phase==='exchange')return new Promise(r=>release=r);return new Response(JSON.stringify(next));}if(url.endsWith('/apps'))return new Promise(r=>release=r);throw Error('unexpected fetch');});
 const pending=f.run("callback(new URLSearchParams({state:'state',code:'code'}))");await new Promise(r=>setImmediate(r));f.event('pagehide');
 if(phase==='verify')f.run('releaseVerify({sub:"subject"})');else release(new Response(JSON.stringify(phase==='token'?{access_token:'test.resource.jwt',id_token:'test',expires_in:600}:phase==='apps'?{apps:[]}:next)));
 await assert.rejects(pending,/canceled|取消/);assert.equal(f.run('session'),null);assert.equal(f.run('spaAccess'),null);assert.equal(f.intervals(),0);assert.equal(exchanges,['token','verify'].includes(phase)?0:1);
});
test('logout clears local authority/timer/recovery before a failed remote DELETE',async()=>{
 const f=await fixture();f.run("session={purpose:'console',sessionKey:'test-key',coordinatorEpoch:'epoch'};spaAccess='test-access';spaExpiry=Date.now()+60000;refreshTimer=setInterval(()=>{},20000);recovery=Promise.resolve();busy=false");let reject!:(reason:Error)=>void;let authorization:string|null=null;
 f.fetch(async(_input,options)=>{authorization=new Headers(options?.headers).get('authorization');return new Promise((_r,j)=>reject=j);});const pending=f.run('logoutConsole()');assert.equal(f.run('session'),null);assert.equal(f.run('spaAccess'),null);assert.equal(f.run('recovery'),null);assert.equal(f.intervals(),0);assert.equal(authorization,'Bearer test-key');reject(Error('offline'));await pending;assert.equal(f.run('session'),null);assert.match(f.run("$('message').textContent"),/本机已退出，中央注销未确认/);assert.equal(f.run("$('logout').hidden"),true);
});

for(const invalid of [{purpose:'device'},{user:{id:'other-subject'}},{coordinatorEpoch:'stale'}])test('callback validates a local exchange result before committing authority '+JSON.stringify(invalid),async()=>{
 const f=await fixture();f.run("config={clientId:'SPA',tokenEndpoint:'https://id.example/token',coordination:{epoch:'epoch'}};sessionStorage.getItem=()=>JSON.stringify({state:'state',nonce:'nonce',verifier:'verifier',createdAt:Date.now()});verifyID=async()=>({sub:'subject'})");
 f.fetch(async input=>new Response(JSON.stringify(String(input)==='https://id.example/token'?{access_token:'test.resource.jwt',id_token:'test',expires_in:600}:{purpose:'console',user:{id:'subject'},sessionKey:'test-key',coordinatorEpoch:'epoch',...invalid})));
 await assert.rejects(f.run("callback(new URLSearchParams({state:'state',code:'code'}))"),/会话类型无效/);assert.equal(f.run('session'),null);assert.equal(f.run('spaAccess'),null);assert.equal(f.intervals(),0);
});

const seed="catalogReady=true;apps=[{id:'Core',name:'Core'},{id:'Other',name:'Other'}];session={purpose:'console',user:{id:'FixtureUser'},sessionKey:'FixtureSession',coordinatorEpoch:'epoch'};$('app-select').value='Core';$('device-subtitle').textContent='1 台在线';$('refreshed').textContent='previous';$('devices').append(node('article','device','old-online'));";
const snapshot=(name='Current')=>({devices:[{id:'FixtureDevice',name,online:true,lastSeen:'2026-11-10T00:00:00.000Z',ports:[{type:'sync',transport:'tcp',port:1234}]}]});
for(const kind of ['network','permission','capacity','json','shape','oversize'])test('complete directory '+kind+' failure seals old online before safe notification',async()=>{
 const f=await fixture();f.run(seed);let release!:(r:Response)=>void,reject!:(e:Error)=>void;
 f.fetch(()=>new Promise((a,b)=>{release=a;reject=b;}));const pending=f.run('refreshDevices()');
 assert.equal(f.run("$('device-subtitle').textContent"),'状态未知');assert.equal(f.run("$('refreshed').textContent"),'');
 if(kind==='network')reject(Error('private-provider-detail'));
 else if(kind==='permission')release(Response.json({error:{code:'FORBIDDEN',message:'private-provider-detail'}},{status:403}));
 else if(kind==='capacity')release(Response.json({error:{code:'DIRECTORY_TOO_LARGE',message:'private-provider-detail'}},{status:413}));
 else if(kind==='json')release(new Response('{private-provider-detail'));
 else if(kind==='oversize')release(new Response(' '.repeat(2*1024*1024+1)));
 else release(Response.json({devices:[...snapshot().devices,{id:'broken',name:'invalid'}]}));
 await pending;assert.equal(f.run("$('device-subtitle').textContent"),'状态未知');
 assert.equal(f.run("$('devices').children.some(n=>n.className==='device')"),false);
 assert.doesNotMatch(f.run("$('message').textContent"),/private-provider-detail/);
});
for(const stale of ['success','403','401'])test('product switch fences stale '+stale+' without replacing current cards or session',async()=>{
 const f=await fixture();f.run(seed);let old!:(r:Response)=>void;let calls=0;
 f.fetch(async()=>++calls===1?new Promise(r=>old=r):Response.json(snapshot('ProductTwo')));
 const pending=f.run('refreshDevices()');f.run("$('app-select').value='Other'");await f.run("$('app-select').listeners.change()");
 if(stale==='success')old(Response.json(snapshot('OldProduct')));else old(Response.json({error:{message:'raw-private'}},{status:Number(stale)}));
 await pending;assert.ok(f.run('session'));assert.equal(f.run("$('app-select').value"),'Other');
 assert.equal(f.run("$('devices').children[0].children[0].children[0].textContent"),'ProductTwo');
});
test('overlapping same-product reads commit only newest; abort is advisory',async()=>{
 const f=await fixture();f.run(seed);const results:Array<(r:Response)=>void>=[];f.fetch(()=>new Promise(r=>results.push(r)));
 const first=f.run('refreshDevices()'),second=f.run('refreshDevices()');results[1]!(Response.json(snapshot('Newest')));await second;
 results[0]!(Response.json(snapshot('Older')));await first;
 assert.equal(f.run("$('devices').children[0].children[0].children[0].textContent"),'Newest');
});
test('apps failure and invalid catalog never leave old online state',async()=>{
 const f=await fixture();f.run(seed);f.fetch(async()=>Response.json({source:'unknown',apps:[]}));
 await assert.rejects(f.run('loadApps()'),(e:any)=>e.code==='INVALID_RESPONSE');assert.equal(f.run("$('device-subtitle').textContent"),'状态未知');
 f.fetch(async()=>Response.json({source:'static',apps:[{id:'Other',name:'Other Product',type:'Native',isDeviceFlow:false}]}));await f.run('loadApps()');
 assert.equal(f.run("$('app-select').value"),'Other');assert.equal(f.run("$('app-id').textContent"),'Other');
});
test('product switch while epoch recovery waits cannot replay prior product',async()=>{
 const f=await fixture();f.run(seed+"config={issuer:'https://id.example/oidc',clientId:'SPA',coordination:{protocol:'memory-v1',epoch:'epoch'}};spaAccess='FixtureAccess';spaExpiry=Date.now()+60000");
 let resolveConfig!:(r:Response)=>void;let oldReads=0;
 f.fetch(async input=>{const url=String(input);if(url.endsWith('/console/auth/config'))return new Promise(r=>resolveConfig=r);if(url.endsWith('/console/auth/exchange'))return Response.json({purpose:'console',user:{id:'FixtureUser'},sessionKey:'FixtureSession',coordinatorEpoch:'new'});if(url.includes('appId=Core')){oldReads++;return Response.json(snapshot(),{headers:{'X-CyberGrid-Epoch':'new'}});}return Response.json(snapshot('Other'));});
 const pending=f.run('refreshDevices()');await new Promise(r=>setImmediate(r));f.run("$('app-select').value='Other'");const selected=f.run("$('app-select').listeners.change()");
 resolveConfig(Response.json({issuer:'https://id.example/oidc',clientId:'SPA',coordination:{protocol:'memory-v1',epoch:'new'}}));await Promise.all([pending,selected]);
 assert.equal(oldReads,1);assert.equal(f.run("$('devices').children[0].children[0].children[0].textContent"),'Other');
});
test('old denied read cannot clear a later login session',async()=>{
 const f=await fixture();f.run(seed);let release!:(r:Response)=>void;f.fetch(()=>new Promise(r=>release=r));const pending=f.run('refreshDevices()');
 f.run("clearSession();session={purpose:'console',user:{id:'NewUser'},sessionKey:'FixtureSession',coordinatorEpoch:'new'}");release(Response.json({error:{message:'do-not-display'}},{status:401}));await pending;
 assert.equal(f.run('session.user.id'),'NewUser');assert.doesNotMatch(f.run("$('message').textContent"),/do-not-display/);
});

test('all deploy buttons recommend static products without Management or public-origin requirement',async()=>{
 const expected=['LOGTO_ISSUER','LOGTO_RESOURCE','LOGTO_BROWSER_CLIENT_ID','CYBERGRID_PRODUCTS','CYBERGRID_REDIS_TRANSPORT','KV_REST_API_URL','KV_REST_API_TOKEN'].sort();
 for(const name of ['../../README.md','../public/index.html']){
  const source=await readFile(new URL(name,import.meta.url),'utf8');
  const links=source.match(/https:\/\/vercel\.com\/new[^\s"<>)]*/g)||[];assert.ok(links.length);
  for(const href of links){const url=new URL(href.replaceAll('&amp;','&'));assert.deepEqual(url.searchParams.get('env')!.split(',').sort(),expected);assert.equal(url.searchParams.get('repository-url'),'https://github.com/ywateru/CyberGrid');}
 }
 const page=await readFile(new URL('../public/index.html',import.meta.url),'utf8');assert.match(page,/CYBERGRID_REDIS_TRANSPORT=vercel-kv/);
 const a=await readFile(new URL('../public/console/index.html',import.meta.url),'utf8'),b=await readFile(new URL('../public/console/callback/index.html',import.meta.url),'utf8');assert.equal(a,b);assert.match(a,/canonical 产品/);
});
