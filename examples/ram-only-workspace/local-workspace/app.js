const output=document.querySelector('#output'),status=document.querySelector('#status');
const launch=location.hash.slice(1);history.replaceState(null,'','/');let csrf;
async function post(path,body,protect=true){const r=await fetch(path,{method:'POST',credentials:'same-origin',headers:{'content-type':'application/json',...(protect?{'x-csrf-token':csrf}:{})},body:JSON.stringify(body??{})});if(!r.ok)throw Error('本机会话无效，请从桌面重新打开。');return r.json();}
async function run(f){try{output.textContent=JSON.stringify(await f(),null,2);}catch{status.textContent='操作未确认，请检查当前状态；不会自动重复写入。';}}
(async()=>{try{if(launch)await post('/exchange',{token:launch},false);csrf=(await post('/bootstrap',{},false)).csrf;status.textContent='本地业务可用';await run(()=>post('/state'));}catch{status.textContent='请从 CyberGrid 桌面点击“在浏览器打开”。';}})();
document.querySelector('#read').onclick=()=>run(()=>post('/state'));
document.querySelector('#write').onclick=()=>run(()=>post('/append',{value:document.querySelector('#value').value}));
document.querySelector('#stream').onclick=()=>{const ws=new WebSocket(location.origin.replace('http:','ws:')+'/ws');ws.onopen=()=>{ws.send(JSON.stringify({csrf}));ws.send('本地双向流可用');};ws.onmessage=e=>{output.textContent=e.data;ws.close();};ws.onerror=()=>{status.textContent='双向流未连接';};};
