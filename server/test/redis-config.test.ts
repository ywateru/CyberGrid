import {test} from 'node:test';
import assert from 'node:assert/strict';
import {createServer} from 'node:http';
import {inspect} from 'node:util';
import {redisTransport} from '../src/redis.js';
const tcp={REDIS_URL:'rediss://fixture.invalid:6379'};
const upstash={UPSTASH_REDIS_REST_URL:'https://upstash.fixture.invalid',UPSTASH_REDIS_REST_TOKEN:['fixture','upstash','write'].join('-')};
const kv={KV_REST_API_URL:'https://kv.fixture.invalid',KV_REST_API_TOKEN:['fixture','kv','write'].join('-')};
function accepts(env:NodeJS.ProcessEnv){const adapter=redisTransport(env);adapter.close();}

test('automatic transport accepts one complete group and ignores empty unselected groups',()=>{
 for(const group of [tcp,upstash,kv])accepts(group);
 accepts({...tcp,UPSTASH_REDIS_REST_URL:'',UPSTASH_REDIS_REST_TOKEN:'',KV_REST_API_URL:'',KV_REST_API_TOKEN:''});
 accepts({...kv,KV_URL:'unused://fixture',KV_REST_API_READ_ONLY_TOKEN:['fixture','read','only'].join('-')});
});
test('automatic transport rejects incomplete or ambiguous groups and never adopts KV aliases or read-only credentials',()=>{
 for(const env of [{},{REDIS_URL:''},{UPSTASH_REDIS_REST_URL:upstash.UPSTASH_REDIS_REST_URL},{UPSTASH_REDIS_REST_TOKEN:upstash.UPSTASH_REDIS_REST_TOKEN},{KV_REST_API_URL:kv.KV_REST_API_URL},{KV_REST_API_TOKEN:kv.KV_REST_API_TOKEN},{...tcp,...kv},{...upstash,...kv},{...tcp,...upstash,...kv},{...tcp,KV_REST_API_TOKEN:kv.KV_REST_API_TOKEN},{...kv,UPSTASH_REDIS_REST_URL:upstash.UPSTASH_REDIS_REST_URL},{KV_URL:tcp.REDIS_URL},{KV_REST_API_URL:kv.KV_REST_API_URL,KV_REST_API_READ_ONLY_TOKEN:['fixture','read','only'].join('-')}])assert.throws(()=>accepts(env));
});
test('explicit selector reads only the selected group even when other groups are invalid or partial',()=>{
 for(const [selector,group] of [['tcp',tcp],['upstash-rest',upstash],['vercel-kv',kv]] as const){
  const allowed=new Set(['CYBERGRID_REDIS_TRANSPORT',...Object.keys(group)]);
  accepts(new Proxy({...group,CYBERGRID_REDIS_TRANSPORT:selector},{get(target,key){assert(allowed.has(String(key)),'unselected configuration must not be read');return Reflect.get(target,key);}}));
  accepts({...tcp,...upstash,...kv,...group,CYBERGRID_REDIS_TRANSPORT:selector});
 }
 accepts({...kv,CYBERGRID_REDIS_TRANSPORT:'vercel-kv',REDIS_URL:'invalid',UPSTASH_REDIS_REST_TOKEN:'partial'});
});
test('unknown or invalid selected group fails without fallback and errors omit credential values',()=>{
 for(const selector of ['', 'KV','vercel-kv ', ' upstash-rest','auto','__proto__'])assert.throws(()=>accepts({...kv,CYBERGRID_REDIS_TRANSPORT:selector}),{message:'Invalid CYBERGRID_REDIS_TRANSPORT'});
 for(const env of [
  {...kv,CYBERGRID_REDIS_TRANSPORT:'tcp'},
  {...kv,CYBERGRID_REDIS_TRANSPORT:'upstash-rest',UPSTASH_REDIS_REST_URL:upstash.UPSTASH_REDIS_REST_URL},
  {...upstash,CYBERGRID_REDIS_TRANSPORT:'vercel-kv',KV_REST_API_URL:kv.KV_REST_API_URL,KV_REST_API_READ_ONLY_TOKEN:['fixture','read','only'].join('-')},
  {...upstash,CYBERGRID_REDIS_TRANSPORT:'vercel-kv',KV_REST_API_URL:'https://fixture.invalid',KV_REST_API_TOKEN:''},
 ])assert.throws(()=>accepts(env),{message:'Selected Redis transport configuration is incomplete'});
 const marker=['fixture','private','marker'].join('-');
 assert.throws(()=>accepts({...kv,CYBERGRID_REDIS_TRANSPORT:'tcp',REDIS_URL:'bad-url-'+marker}),(error:unknown)=>error instanceof Error&&!inspect(error).includes(marker)&&!JSON.stringify(error).includes(marker));
 for(const [url,token] of [['not-a-url-'+marker,'fixture'],['http://'+marker+'.invalid','fixture'],['https://user:'+marker+'@fixture.invalid','fixture'],['https://fixture.invalid/path/'+marker,'fixture'],['https://fixture.invalid?'+marker,'fixture'],['https://fixture.invalid#'+marker,'fixture'],['https://fixture.invalid',' '+marker],['https://fixture.invalid',marker+'\n'],[' https://fixture.invalid','fixture'],['https://fixture.invalid',marker+'\u2603']]){
  assert.throws(()=>accepts({...upstash,CYBERGRID_REDIS_TRANSPORT:'vercel-kv',KV_REST_API_URL:url,KV_REST_API_TOKEN:token}),(error:unknown)=>error instanceof Error&&!String(error.stack).includes(marker)&&!JSON.stringify(error).includes(marker)&&!inspect(error).includes(marker));
 }
});
test('both REST groups execute the existing write-routed EVAL adapter with the selected full token',async t=>{
 const received:{authorization:string|undefined;method:string|undefined;body:unknown}[]=[];
 const server=createServer((req,res)=>{let raw='';req.setEncoding('utf8');req.on('data',chunk=>raw+=chunk);req.on('end',()=>{received.push({authorization:req.headers.authorization,method:req.method,body:JSON.parse(raw)});res.writeHead(200,{'content-type':'application/json'});res.end(JSON.stringify({result:'ok'}));});});
 await new Promise<void>(resolve=>server.listen(0,'127.0.0.1',resolve));t.after(async()=>{server.closeAllConnections();await new Promise<void>(resolve=>server.close(()=>resolve()));});
 const address=server.address() as {port:number},realFetch=globalThis.fetch,targets:string[]=[];
 // Preserve production HTTPS validation; route fixture fetches to a local HTTP recorder.
 t.mock.method(globalThis,'fetch',async(input:Parameters<typeof fetch>[0],init?:RequestInit)=>{targets.push(String(input));assert.equal(init?.redirect,'error');assert.equal(init?.credentials,'omit');assert(init?.signal instanceof AbortSignal);return realFetch('http://127.0.0.1:'+address.port,init);});
 for(const selector of ['upstash-rest','vercel-kv']){
  const adapter=redisTransport({...tcp,...upstash,...kv,KV_REST_API_READ_ONLY_TOKEN:['fixture','read','only'].join('-'),CYBERGRID_REDIS_TRANSPORT:selector});
  assert.equal(await adapter.eval('return ARGV[1]','{fixture}:state',['safe-argument']),'ok');adapter.close();
 }
 assert.deepEqual(targets,[upstash.UPSTASH_REDIS_REST_URL+'/',kv.KV_REST_API_URL+'/']);
 assert.deepEqual(received,[upstash.UPSTASH_REDIS_REST_TOKEN,kv.KV_REST_API_TOKEN].map(token=>({authorization:'Bearer '+token,method:'POST',body:['EVAL','return ARGV[1]','1','{fixture}:state','safe-argument']})));
});
