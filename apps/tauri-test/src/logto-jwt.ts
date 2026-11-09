import type {JwtVerifier} from '@logto/client';
import {createLocalJWKSet, jwtVerify, type JSONWebKeySet} from 'jose';

/** Public Logto verifier extension; preserves the default verifier's checks.
 * Native fetch pins and bounds the JWKS response. No WebView network/CORS needed.
 */
export function nativeJwtVerifier(
  issuer: string,
  appId: string,
  getConfiguration: () => Promise<{issuer:string;jwksUri:string}>,
  transport: typeof fetch,
): JwtVerifier {
  let keys: Promise<ReturnType<typeof createLocalJWKSet>> | undefined;
  const getKeys = () => keys ??= (async () => {
    const configuration = await getConfiguration();
    if (configuration.issuer !== issuer) throw new Error('Trusted issuer mismatch');
    const response = await transport(configuration.jwksUri, {method:'GET'});
    const body = await response.text();
    if (body.length > 131072) throw new Error('JWKS response limit');
    const jwks: JSONWebKeySet = JSON.parse(body);
    if (!Array.isArray(jwks.keys) || jwks.keys.length < 1 || jwks.keys.length > 32) throw new Error('JWKS key limit');
    return createLocalJWKSet(jwks);
  })().catch(error => { keys = undefined; throw error; });
  return {
    verifyIdToken: async idToken => {
      const result = await jwtVerify(idToken, await getKeys(), {audience:appId,issuer,clockTolerance:300,requiredClaims:['sub','iat','exp']});
      if (typeof result.payload.sub !== 'string' || result.payload.sub.trim().length === 0) throw new Error('ID token subject required');
      if (Math.abs((result.payload.iat ?? 0) - Date.now()/1000) > 300) throw new Error('ID token issued-at window');
    },
  };
}
