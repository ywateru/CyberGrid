import {test} from 'node:test';
import assert from 'node:assert/strict';
import {spawn,type ChildProcess} from 'node:child_process';
import {once} from 'node:events';
import {createServer} from 'node:net';
import {redisFixture} from './redis-fixture.js';
async function port(){const s=createServer();await new Promise<void>(r=>s.listen(0,'127.0.0.1',r));const p=(s.address() as {port:number}).port;await new Promise<void>(r=>s.close(()=>r()));return p;}
test('actual stateless Node processes coexist and SIGKILL/restart preserves Redis epoch/session',{timeout:20000},async t=>{
 const f=await redisFixture(t);if(!f)return;const children:ChildProcess[]=[];t.after(async()=>{for(const c of children)if(c.exitCode===null&&c.signalCode===null){c.kill('SIGTERM');await once(c,'exit');}});
 const launch=async()=>{const p=await port(),env:NodeJS.ProcessEnv={...process.env,NODE_ENV:'test',PORT:String(p),CYBERGRID_BIND:'127.0.0.1',REDIS_URL:f.url,CYBERGRID_REDIS_PREFIX:f.prefix,CYBERGRID_PUBLIC_ORIGIN:'https://grid.example',LOGTO_ISSUER:'https://fixture.invalid/oidc',LOGTO_BROWSER_CLIENT_ID:'FixtureSPA',LOGTO_MANAGEMENT_CLIENT_ID:'FixtureDirectory',LOGTO_MANAGEMENT_CLIENT_SECRET:'fixture-unused'};delete env.UPSTASH_REDIS_REST_URL;delete env.UPSTASH_REDIS_REST_TOKEN;
 const c=spawn(process.execPath,['--import','tsx','src/main.ts'],{env,stdio:['ignore','pipe','pipe']});children.push(c);c.stderr!.resume();await new Promise<void>((resolve,reject)=>{let out='';const timer=setTimeout(()=>reject(Error('Node startup deadline')),8000);c.stdout!.on('data',chunk=>{out+=String(chunk);if(out.includes('Node service listening')){clearTimeout(timer);resolve();}});c.once('exit',()=>{clearTimeout(timer);reject(Error('Node exited before ready'));});});return {c,base:'http://127.0.0.1:'+p};};
 const a=await launch(),b=await launch();const epoch=async(base:string)=>(await fetch(base+'/api/v1/console/auth/config')).headers.get('x-cybergrid-epoch');assert.equal(await epoch(a.base),await epoch(b.base));
 const s=f.store({},f.prefix);await s.ready();const registration=await s.register('fixture-user',null,'',undefined,'a5a8fef1-9765-4590-9155-fb661c30675e',0);const initial=await epoch(a.base);
 a.c.kill('SIGKILL');await once(a.c,'exit');const restarted=await launch();assert.equal(await epoch(restarted.base),initial);const response=await fetch(restarted.base+'/api/v1/console/session',{method:'DELETE',headers:{authorization:'Bearer '+registration.sessionKey}});assert.equal(response.status,200);assert.equal(await s.session(registration.session.hash),undefined);
});
