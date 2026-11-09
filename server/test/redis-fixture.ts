import {spawn} from 'node:child_process';
import {createServer} from 'node:net';
import {mkdtemp,rm} from 'node:fs/promises';
import {tmpdir} from 'node:os';
import {join} from 'node:path';
import {randomUUID} from 'node:crypto';
import {createClient} from 'redis';
import {RedisStore,redisTransport} from '../src/redis.js';
const delay=(ms:number)=>new Promise(r=>setTimeout(r,ms));
async function port(){const s=createServer();await new Promise<void>(r=>s.listen(0,'127.0.0.1',r));const p=(s.address() as {port:number}).port;await new Promise<void>(r=>s.close(()=>r()));return p;}
export async function redisFixture(t:any){
 const p=await port(),dir=await mkdtemp(join(tmpdir(),'cybergrid-real-redis-'));let failed=false;
 const process=spawn(globalThis.process.env.REDIS_TEST_SERVER??'redis-server',['--bind','127.0.0.1','--port',String(p),'--dir',dir,'--appendonly','yes','--appendfsync','always','--save','','--maxmemory-policy','noeviction'],{stdio:'ignore'});process.on('error',()=>{failed=true;});
 const url=`redis://127.0.0.1:${p}`,client=createClient({url,socket:{reconnectStrategy:false,connectTimeout:100}});client.on('error',()=>{});
 for(let i=0;i<100;i++){if(failed){await rm(dir,{recursive:true,force:true});if(globalThis.process.env.CI==='true'||globalThis.process.env.REQUIRE_REDIS==='1')throw Error('Real Redis required: install redis-server or set REDIS_TEST_SERVER');t.skip('Install redis-server or set REDIS_TEST_SERVER');return;}try{await client.connect();break;}catch{await delay(25);}}if(!client.isReady)throw Error('Redis startup failed');
 const stores:RedisStore[]=[];const store=(limits={},prefix='test-'+randomUUID())=>{const s=new RedisStore(redisTransport({REDIS_URL:url}),prefix,limits);stores.push(s);return s;};
 t.after(async()=>{for(const s of stores)s.close();if(client.isOpen)client.destroy();process.kill('SIGTERM');await new Promise<void>(r=>process.once('exit',()=>r()));await rm(dir,{recursive:true,force:true});});return {url,client,store,prefix:'test-'+randomUUID()};
}
