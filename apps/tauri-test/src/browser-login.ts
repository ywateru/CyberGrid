import LogtoClient, {decodeAccessToken, type ClientAdapter, type PersistKey, type StorageKey} from '@logto/client';
import {generateCodeChallenge, generateCodeVerifier, generateState} from '@logto/browser';
import {invoke} from '@tauri-apps/api/core';
import {nativeJwtVerifier} from './logto-jwt';
import {browserLoginFailure, browserStartFailureMessage} from './browser-start-error';

type LabConfig = {baseUrl:string;appId:string;deviceName:string;forceFallback:boolean;cloudflaredPath:string};
type Transaction = {transactionId:string;generation:number;platform:'desktop'|'android';endpoint:string;issuer:string;appId:string;resource:string;scope:string;redirectUri:string};
type NativeResponse = {status:number;headers:Record<string,string>;body:string};
type Handoff = 'OwnedPersisted' | 'OwnedPending';
type Owner = {transaction:Transaction;client:LogtoClient;memory:Map<string,string>};
let operation = 0;
let active = false;
// A failed IPC acknowledgement must not discard the only remaining JS owner.
// Native pending escrow survives independently and blocks conflicting starts.
const unconfirmed = new Map<string, Owner>();
const safeFailure = browserLoginFailure;

async function trace(phase:string,features?:{abortTimeout:boolean;abortAny:boolean;webCrypto:boolean}):Promise<void>{
  try {await invoke('browser_trace',{phase,features});}catch{/* Diagnostics never change grant ownership. */}
}
async function cleanup(owner: Owner): Promise<void> {
  await invoke('browser_cancel', {transactionId:owner.transaction.transactionId});
  await owner.client.clearAllTokens();
  owner.memory.clear();
  unconfirmed.delete(owner.transaction.transactionId);
}

export function invalidateBrowserLogin(): void { operation++; }

export async function startBrowserLogin(config: LabConfig): Promise<void> {
  if (active) throw new Error('已有网页登录正在执行，请先取消或等待完成。');
  active = true;
  const current = ++operation;
  let transaction: Transaction | undefined;
  let owner: Owner | undefined;
  let transferred = false;
  let failure = safeFailure;
  try {
    // Retry native ownership acknowledgement before beginning another grant.
    for (const retained of unconfirmed.values()) await cleanup(retained);
    await trace('begin-await',{abortTimeout:typeof AbortSignal.timeout === 'function',abortAny:typeof AbortSignal.any === 'function',webCrypto:Boolean(globalThis.crypto?.subtle)});
    transaction = await invoke<Transaction>('begin_web_login', {config});
    await trace('begin-ready');
    if (current !== operation) throw new Error(safeFailure);
    if (transaction.platform === 'android') return; // Official Android SDK owns this transaction.
    if (transaction.platform !== 'desktop') throw new Error(safeFailure);
    const desktop = transaction;
    const memory = new Map<string,string>();
    const assertCurrent = () => { if (current !== operation) throw new Error('登录已取消。'); };
    const transport: typeof fetch = async (input, init) => {
      assertCurrent();
      const request = new Request(input, init);
      const headers: Record<string,string> = {};
      request.headers.forEach((value, name) => { headers[name] = value; });
      const body = ['GET','HEAD'].includes(request.method) ? undefined : await request.text();
      const response = await invoke<NativeResponse>('browser_fetch', {transactionId:desktop.transactionId, request:{url:request.url,method:request.method,headers,body}});
      // Native transport journals minted/rotated grants before returning here.
      assertCurrent();
      // The official default requester logs raw non-2xx bodies. Reject at this
      // adapter boundary so OAuth error responses cannot enter ordinary logs.
      if (response.status < 200 || response.status >= 300) throw new Error(safeFailure);
      return new Response([204,205,304].includes(response.status) ? null : response.body, {status:response.status,headers:response.headers});
    };
    const adapter: ClientAdapter = {
      storage:{
        getItem:async (key: StorageKey | PersistKey) => memory.get(key) ?? null,
        setItem:async (key: StorageKey | PersistKey, value: string) => { assertCurrent(); memory.set(key,value); },
        removeItem:async (key: StorageKey | PersistKey) => { memory.delete(key); },
      },
      fetch:transport,
      navigate:async (url, parameters) => {
        assertCurrent();
        if (parameters.for !== 'sign-in') throw new Error(safeFailure);
        await trace('open-await');
        await invoke('browser_open', {transactionId:desktop.transactionId,url});
        await trace('open-ready');
      },
      generateState:async()=>{await trace('pkce-state');return generateState();},
      generateCodeVerifier:async()=>{await trace('pkce-verifier');return generateCodeVerifier();},
      generateCodeChallenge:async verifier=>{await trace('pkce-challenge');return generateCodeChallenge(verifier);},
    };
    const client = new LogtoClient({endpoint:transaction.endpoint,appId:transaction.appId,resources:[transaction.resource],scopes:transaction.scope.split(/\s+/).filter(Boolean),requestTimeoutMs:30000},adapter,
      instance => nativeJwtVerifier(desktop.issuer,desktop.appId,() => instance.getOidcConfig(),transport));
    owner = {transaction,client,memory};
    await trace('client-created');
    await trace('sdk-signin');
    await client.signIn({redirectUri:transaction.redirectUri});
    await trace('callback-await');
    const callbackUri = await invoke<string>('browser_wait_callback', {transactionId:transaction.transactionId});
    assertCurrent();
    // Native receiver checks exact loopback origin/path; official SDK checks state/PKCE.
    await trace('callback-ready');
    const callback = new URL(callbackUri);
    await trace(callback.searchParams.has('code') ? 'callback-has-code' : 'callback-no-code');
    await trace(callback.searchParams.has('state') ? 'callback-has-state' : 'callback-no-state');
    const callbackError = callback.searchParams.get('error');
    const knownErrors = ['invalid_request','invalid_scope','invalid_target','unauthorized_client','access_denied','unsupported_response_type','server_error','temporarily_unavailable','interaction_required','login_required','consent_required'];
    if (callbackError) await trace(knownErrors.includes(callbackError) ? `callback-error-${callbackError}` : 'callback-error-other');
    await trace('sdk-callback');
    try { await client.handleSignInCallback(callbackUri); }
    catch (error) {
      const code = typeof error === 'object' && error !== null && 'code' in error ? error.code : undefined;
      const callbacks:Record<string,string> = {'sign_in_session.not_found':'sdk-session-missing','sign_in_session.invalid':'sdk-session-invalid','callback_uri_verification.redirect_uri_mismatched':'sdk-redirect-mismatch','callback_uri_verification.error_found':'sdk-error-callback','callback_uri_verification.missing_state':'sdk-state-missing','callback_uri_verification.state_mismatched':'sdk-state-mismatch','callback_uri_verification.missing_code':'sdk-code-missing'};
      await trace(typeof code === 'string' && Object.hasOwn(callbacks,code) ? callbacks[code] : 'sdk-callback-other');
      throw new Error(safeFailure);
    }
    assertCurrent();
    await trace('resource-await');
    const accessToken = await client.getAccessToken(transaction.resource);
    // This explicit resource call can rotate RT. Export RT only after it finishes.
    const refreshToken = await client.getRefreshToken();
    const idToken = await client.getIdToken();
    const claims = decodeAccessToken(accessToken);
    const expiresIn = Math.floor(Number(claims.exp) - Date.now()/1000);
    if (!refreshToken || !Number.isFinite(expiresIn) || expiresIn < 1 || typeof claims.scope !== 'string') throw new Error(safeFailure);
    assertCurrent();
    await trace('commit-await');
    const result = await invoke<Handoff>('browser_commit', {transactionId:transaction.transactionId,tokens:{accessToken,refreshToken,idToken:idToken ?? undefined,expiresIn,tokenType:'Bearer',scope:claims.scope}});
    if (result !== 'OwnedPersisted' && result !== 'OwnedPending') throw new Error(safeFailure);
    transferred = true;
    await trace('commit-ready');
    // clearAllTokens is local cleanup, not SDK signOut/revocation. Rust is now sole owner.
    await client.clearAllTokens();
    memory.clear();
    if (result === 'OwnedPending') {
      failure = '授权已由本机接管，但私有保存尚未完成。请恢复存储后重试恢复或撤销；此时尚未登录就绪。';
      throw new Error(failure);
    }
  } catch (error) {
    if (!transaction) failure = browserStartFailureMessage(error);
    await trace('failed');
    if (owner && !transferred) {
      unconfirmed.set(owner.transaction.transactionId, owner);
      try { await cleanup(owner); } catch { /* Retain RAM until native ownership is confirmed. */ }
    } else if (!owner && transaction) {
      try { await invoke('browser_cancel', {transactionId:transaction.transactionId}); } catch { /* Native pending exclusion remains authoritative. */ }
    }
    throw new Error(current !== operation ? '登录已取消；待处理授权由本机安全收尾。' : failure);
  } finally { active = false; }
}
