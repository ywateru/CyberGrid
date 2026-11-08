import {test} from 'node:test';
import assert from 'node:assert/strict';
import {generateKeyPair,SignJWT,exportJWK,createLocalJWKSet} from 'jose';
import {LogtoIdentity} from '../src/identity.js';
import type {AuthConfiguration} from '../src/types.js';
const config:AuthConfiguration={provider:'logto',issuer:'https://tenant.example/oidc',clientId:'native-client',resource:'https://cybergrid.aixgg.com/api',scope:'openid connect',deviceAuthorizationEndpoint:'https://tenant.example/oidc/device/auth',tokenEndpoint:'https://tenant.example/oidc/token',authorizationEndpoint:'https://tenant.example/oidc/auth',revocationEndpoint:'https://tenant.example/oidc/token/revocation',redirectUri:'https://cybergrid.aixgg.com/console/callback/',jwksUri:'https://tenant.example/oidc/jwks'};
test('exchange verifier pins issuer/audience/algorithm/client/user/expiry and whole-word connect permission',async()=>{
 const keys=await generateKeyPair('ES384');const jwk={...await exportJWK(keys.publicKey),kid:'test',alg:'ES384'};
 const identity=new LogtoIdentity(config,['ES384'],createLocalJWKSet({keys:[jwk]}));
 const now=Math.floor(Date.now()/1000);
 const claims={sub:'logto-user',iss:config.issuer,aud:config.resource,client_id:config.clientId,scope:'openid connect',iat:now,exp:now+60};
 const sign=(changes:Record<string,unknown>={})=>new SignJWT({...claims,...changes}).setProtectedHeader({alg:'ES384',kid:'test',typ:'JWT'}).sign(keys.privateKey);
 assert.deepEqual(await identity.verify(await sign()),{id:'logto-user',clientId:config.clientId});
 assert.deepEqual(await identity.verify(await sign({client_id:'browser-client',email:'user@example.test'})),{id:'logto-user',clientId:'browser-client',email:'user@example.test'});
 for(const changes of [{iss:'https://other.example/oidc'},{aud:config.clientId},{exp:now-30},{scope:'openid disconnect'},{client_id:undefined},{sub:config.clientId},{sub:undefined},{gty:'client_credentials'},{azp:'conflicting-client'},{iat:undefined}])await assert.rejects(()=>sign(changes).then(t=>identity.verify(t)),e=>(e as any).code==='INVALID_IDENTITY');
 await assert.rejects(()=>identity.verify('pat_not_a_user_token'));
 const other=await generateKeyPair('ES384');const forged=await new SignJWT(claims).setProtectedHeader({alg:'ES384',kid:'test'}).sign(other.privateKey);await assert.rejects(()=>identity.verify(forged));
});
