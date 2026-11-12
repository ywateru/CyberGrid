// Local acceptance fixture: real two-handler Node/Redis/JWK stack; no user accounts.
import {createServer} from 'node:http';
import {createHash,randomUUID} from 'node:crypto';
import {generateKeyPair,exportJWK,SignJWT,createLocalJWKSet} from '../../server/node_modules/jose/dist/webapi/index.js';
import {redisFixture} from '../../server/test/redis-fixture.js';
import {RedisStore,redisTransport} from '../../server/src/redis.js';
import {LogtoIdentity} from '../../server/src/identity.js';
import {coordinatorAuthConfiguration} from '../../server/src/deployment-config.js';
import {ProductPolicy} from '../../server/src/products.js';
import {EndpointPolicy} from '../../server/src/endpoints.js';
import {createService} from '../../server/src/service.js';
import {signAdmission} from '../../server/src/admission.js';
import {serveAPI} from '../../server/src/http.js';
const cleanup:(()=>Promise<unknown>|void)[]=[];
const f=await redisFixture({after:(fn:()=>Promise<unknown>)=>cleanup.push(fn),skip:()=>{throw Error('Real Redis required');}});if(!f)throw Error('Redis unavailable');
const keys=await generateKeyPair('ES384'),jwk={...await exportJWK(keys.publicKey),kid:'acceptance',alg:'ES384'};
const config=coordinatorAuthConfiguration({LOGTO_ISSUER:'https://fixture.example.test/oidc',LOGTO_RESOURCE:'https://fixture.example.test/api',LOGTO_BROWSER_CLIENT_ID:'browser'});
let identity=new LogtoIdentity(config,['ES384'],createLocalJWKSet({keys:[jwk]}));
const tokens:Record<string,string>={};for(const [name,client,sub] of [['desktop','desktop','owner'],['mobile','mobile','owner'],['stranger','desktop','stranger']])tokens[name!]=await new SignJWT({client_id:client,scope:'openid offline_access connect'}).setSubject(sub!).setIssuer(config.issuer).setAudience(config.resource).setIssuedAt().setExpirationTime('1h').setProtectedHeader({alg:'ES384',kid:'acceptance'}).sign(keys.privateKey);
let products=new ProductPolicy(JSON.stringify({version:1,products:[{id:'desktop',name:'Workspace',nativeClientIds:['desktop','mobile'],browserClientIds:['browser']}]}));
let endpoints=new EndpointPolicy(),mode:'finite-v2'|'legacy-v1'='finite-v2';let suffix=0;let handles:ReturnType<typeof createService>[]=[];const stores:RedisStore[]=[];
async function rebuild(){const group=[0,1].map(()=>new RedisStore(redisTransport({REDIS_URL:f!.url}),f!.prefix+'-'+suffix,{},mode));stores.push(...group);await Promise.all(group.map(s=>s.ready()));handles=group.map(store=>createService({store,identity,directory:products,products,endpointPolicy:endpoints,browserClientId:'browser',authRateLimit:1000}));}
await rebuild();let losses:Record<string,number>={},held:string|null=null,release:(()=>void)|null=null;const calls:Record<string,string[]>={};const registeredIds:string[]=[];const outcomes:Record<string,string[]>={};let rawConfigPatch:Record<string,unknown>={};let expiredClaim:string|null=null;let directoryFailure=false,configFailure=false;let configOmit:string[]=[];
const servers=await Promise.all([0,1].map(async index=>{const server=createServer((req,res)=>void serveAPI(req,res,async(request)=>{
 const path=new URL(request.url).pathname;
 if(path==='/test/stats')return Response.json({calls,held,registeredIds,outcomes});
 if(path==='/test/control'){const body=await request.json() as any;
  if(body.products){products=new ProductPolicy(JSON.stringify(body.products));await rebuild();}
  if(body.rejectIdentity){const replacement=await generateKeyPair('ES384');identity=new LogtoIdentity(config,['ES384'],createLocalJWKSet({keys:[{...await exportJWK(replacement.publicKey),kid:'acceptance',alg:'ES384'}]}));await rebuild();}
  if(body.expiredClaim!==undefined)expiredClaim=body.expiredClaim;if(body.directoryFailure!==undefined)directoryFailure=body.directoryFailure;if(body.configFailure!==undefined)configFailure=body.configFailure;
  if(body.losses)losses=body.losses;if(body.hold)held=body.hold;if(body.release){held=null;release?.();release=null;}
  if(body.policy){endpoints=new EndpointPolicy(JSON.stringify(body.policy));await rebuild();}if(body.configPatch)rawConfigPatch=body.configPatch;if(body.configOmit)configOmit=body.configOmit;
  if(body.epoch){suffix++;await rebuild();}if(body.mode){mode=body.mode;suffix++;await rebuild();}
  return Response.json({ok:true});
 }
 if(path==='/test/stop'){queueMicrotask(()=>void shutdown());return Response.json({ok:true});}
 const operation=path.endsWith('/admit')?'mint':path.endsWith('/exchange')?'register':path.endsWith('/challenge')?'challenge':null;
 if(operation){const bytes=await request.clone().text();(calls[operation]??=[]).push(createHash('sha256').update(bytes).digest('hex'));}
 if(directoryFailure&&path.endsWith('/directory')||configFailure&&path.endsWith('/auth/config'))return Response.json({error:{code:'COORDINATOR_UNAVAILABLE',message:'fixture'}},{status:503});
 let response=await handles[index]!(request);
 // Expired signed fixture claims exercise the actual verifier + Redis TIME rejection,
 // without pretending that 24h elapsed or altering the production coordinator clock.
 if(response.ok&&(expiredClaim==='challenge'&&operation==='challenge'||expiredClaim==='ticket'&&operation==='mint')){
  const kind=operation==='challenge'?'challenge':'ticket',field=kind==='challenge'?'challenge':'admissionTicket';
  const value=await response.json() as any,claim=JSON.parse(Buffer.from(value[field].split('.')[0],'base64url').toString());
  const ttl=kind==='challenge'?120000:86400000,notAfter=Date.now()-1000;
  const meta=JSON.parse((await f!.client.hGet('{'+f!.prefix+'-'+suffix+'}:meta','state'))!);
  value[field]=signAdmission(Buffer.from(meta.seed,'base64url'),kind,{epoch:claim[1],userId:claim[2],productId:claim[3],authClientId:claim[4],keyHash:claim[5],domain:claim[6],...(kind==='challenge'?{nonce:claim[7]}:{}),issuedAt:notAfter-ttl,notAfter});
  value.expiresAt=new Date(notAfter).toISOString();response=Response.json(value);
 }
 if(operation&&!response.ok){const error=await response.clone().json() as any;(outcomes[operation]??=[]).push(String(error?.error?.code));}
 if(operation==='register'&&response.ok){const v=await response.clone().json() as any;if(v.deviceId)registeredIds.push(v.deviceId);}
 if(path.endsWith('/auth/config')&&(Object.keys(rawConfigPatch).length||configOmit.length)){const value={...await response.json(),...rawConfigPatch};for(const key of configOmit)delete value[key];response=new Response(JSON.stringify(value),{status:response.status,headers:response.headers});}
 if(operation&&held===operation){await new Promise<void>(resolve=>{release=resolve;});}
 if(operation&&(losses[operation]??0)>0){losses[operation]!--;return Response.json({error:{code:'SIMULATED_RESPONSE_LOSS',message:'fixture'}},{status:599});}return response;
 }));await new Promise<void>(r=>server.listen(0,'127.0.0.1',r));return server;}));
let stopping=false;async function shutdown(){if(stopping)return;stopping=true;release?.();for(const s of servers)s.closeAllConnections();await Promise.all(servers.map(s=>new Promise<void>(r=>s.close(()=>r()))));for(const s of stores)s.close();for(const fn of cleanup.reverse())await fn();process.exit(0);}
process.once('SIGTERM',()=>void shutdown());process.once('SIGINT',()=>void shutdown());
console.log(JSON.stringify({left:'http://127.0.0.1:'+(servers[0]!.address() as any).port,right:'http://127.0.0.1:'+(servers[1]!.address() as any).port,tokens}));
process.stdin.resume();process.stdin.once('end',()=>void shutdown());
