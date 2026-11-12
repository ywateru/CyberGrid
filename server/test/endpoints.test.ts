import {test} from 'node:test';
import assert from 'node:assert/strict';
import {createHash,randomBytes} from 'node:crypto';
import {EndpointPolicy,bridges} from '../src/endpoints.js';
const provider={id:'work',origins:['https://remote.example.test:8443','https://remote.example.test'],paths:['/bridge','/']};
const policy=()=>new EndpointPolicy(JSON.stringify({version:1,providers:[provider]}));
export const bridge=(overrides:Record<string,unknown>={})=>({id:'bridge',protocol:'noise.ik.v1',providerId:'quick-tunnel',url:'wss://fixture.trycloudflare.com/remote/v1/ws',installationId:'installation',publicKey:randomBytes(32).toString('base64url'),addressRevision:1,expiresAt:new Date(Date.now()+60000).toISOString(),limits:{maxFrameBytes:4096,maxStreams:4,maxPendingRequests:8},...overrides});
test('endpoint policy canonical hash, sorted sets, explicit authority and exact path',()=>{
 const p=policy(),sorted={...provider,origins:provider.origins.slice().reverse(),paths:provider.paths.slice().reverse()};assert.equal(p.policyVersion,new EndpointPolicy(JSON.stringify({version:1,providers:[sorted]})).policyVersion);
 assert.equal(p.policyVersion,createHash('sha256').update(JSON.stringify(['cybergrid-endpoint-policy-v1',[['work',provider.origins.slice().sort(),provider.paths.slice().sort()]]])).digest('hex'));
 assert.deepEqual(new EndpointPolicy().configuration().providers,[]);assert.equal(p.tunnel('https://remote.example.test:8443/bridge'),'https://remote.example.test:8443/bridge');assert.equal(p.tunnel('https://remote.example.test:8443/',true),'https://remote.example.test:8443');assert.equal(p.bridge('wss://remote.example.test:8443/bridge','work'),'wss://remote.example.test:8443/bridge');assert.throws(()=>p.bridge('wss://remote.example.test:8444/bridge','work'));assert.throws(()=>p.tunnel('https://remote.example.test/bridge',true));
 for(const value of ['https://fixture.trycloudflare.com','https://fixture.trycloudflare.com/'])assert.equal(p.tunnel(value),'https://fixture.trycloudflare.com');assert.equal(p.bridge('wss://fixture.trycloudflare.com/explicit/path','quick-tunnel'),'wss://fixture.trycloudflare.com/explicit/path');
});
test('endpoint policy fails closed on unknown, duplicate, empty or noncanonical configuration',()=>{
 const bad=[{}, {version:2,providers:[provider]},{version:1,providers:[]},{version:1,providers:[provider],extra:true},{version:1,providers:[provider,provider]}, {version:1,providers:[{...provider,id:'quick-tunnel'}]}];
 for(const field of ['origins','paths'])for(const value of [[],[provider[field as 'origins'|'paths'][0],provider[field as 'origins'|'paths'][0]]])bad.push({version:1,providers:[{...provider,[field]:value}]});
 for(const value of ['https://REMOTE.example.test','https://remote.example.test:443','https://remote.example.test/','http://remote.example.test','https://remote.example.test?q','https://u:p@remote.example.test'])bad.push({version:1,providers:[{...provider,origins:[value]}]});
 for(const value of ['/a/../bridge','//bridge','/./bridge','/%62ridge','/bridge?x','/bridge#x','/桥','/a\\b','x','/'+ 'a'.repeat(256)])bad.push({version:1,providers:[{...provider,paths:[value]}]});
 for(const v of bad)assert.throws(()=>new EndpointPolicy(JSON.stringify(v)),JSON.stringify(v));assert.throws(()=>new EndpointPolicy(' '.repeat(32769)));assert.throws(()=>new EndpointPolicy(''));
});
test('endpoint validation rejects original spelling normalization and Quick HTTPS nonroot URLs',()=>{
 const p=policy();assert.equal(p.tunnel('https://'+'a'.repeat(63)+'.trycloudflare.com'),'https://'+'a'.repeat(63)+'.trycloudflare.com');for(const value of ['https://'+'a'.repeat(64)+'.trycloudflare.com','https://FIXTURE.trycloudflare.com/','https://fixture.trycloudflare.com:443/','https://fixture.trycloudflare.com/a/../','https://fixture.trycloudflare.com/%2e/','https://fixture.trycloudflare.com/?','https://fixture.trycloudflare.com/#','https://fixture.trycloudflare.com/bridge','https://fixture.trycloudflare.com\\','https://fixture.trycloudflare.com@evil.test/','https://remote.example.test:8443//bridge','https://remote.example.test/./bridge'])assert.throws(()=>p.tunnel(value),value);
 for(const value of ['wss://fixture.trycloudflare.com:443/bridge','wss://fixture.trycloudflare.com/a/../bridge','wss://fixture.trycloudflare.com/%62ridge','wss://fixture.trycloudflare.com//bridge','wss://fixture.trycloudflare.com/bridge?','wss://fixture.trycloudflare.com/bridge#','wss://fixture.trycloudflare.com','wss://remote.example.test:8443/bridge/'])assert.throws(()=>p.bridge(value,value.includes('remote.')?'work':'quick-tunnel'),value);
});
test('bridge limits, identifiers, canonical key/time and complete shape are mandatory',()=>{
 const b=bridge();assert.equal(bridges([b],policy())[0]!.url,b.url);for(const patch of [{limits:undefined},{limits:{...b.limits,extra:1}},{limits:{...b.limits,maxFrameBytes:65536}},{limits:{...b.limits,maxStreams:33}},{limits:{...b.limits,maxPendingRequests:0}},{id:'a.b'},{protocol:'a b'},{publicKey:b.publicKey+'='},{addressRevision:0},{expiresAt:'2026-11-10T00:00:00Z'},{extra:true}])assert.throws(()=>bridges([{...b,...patch}],policy()));assert.throws(()=>bridges([b,b],policy()));assert.throws(()=>bridges([b,b,b,b,b],policy()));
});
