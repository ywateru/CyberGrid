# RAM-only workspace sample

This sample consumes the real Rust and JS SDKs and the official `@logto/client@3.2.2`. It provides a Tauri 2 desktop owner, independent local business, an owned browser-bridge publisher and a remote SPA. It does not implement or redistribute an external Noise transport. The executable has **no Gateway factory**, and the public Web entry has **no business adapter**. Those entries report remote business unavailable until a deliberately trusted integration is supplied.

The contract is [common-sample](../../docs/common-sample.md). Public source, controlled fixtures, actual external interoperability, cloud login, phone networks and timed acceptance have separate evidence. A successful build or local fixture does not complete integration-010.

## Run and configure

From the repository root:

```sh
npm ci --prefix examples/ram-only-workspace/desktop
npm ci --prefix examples/ram-only-workspace/web
npm --prefix examples/ram-only-workspace/desktop run tauri dev
```

The desktop opens local business immediately, without an account or Internet. Native login uses the system browser with the exact callback `http://127.0.0.1:17896/callback`; register that URI on the real Native Logto app and avoid another Lab occupying the port. The separate app identifier is `tech.tinadev.cybergrid.workspace`.

Native build configuration is public configuration, never credentials:

| Variable | Fixed binding |
| --- | --- |
| `CYBERGRID_SAMPLE_SERVER` | Trusted coordinator origin |
| `CYBERGRID_SAMPLE_ISSUER` | Exact Logto issuer ending in `/oidc` |
| `CYBERGRID_SAMPLE_NATIVE_CLIENT` | Real Native client ID |
| `CYBERGRID_SAMPLE_RESOURCE` | Exact API resource/audience |
| `CYBERGRID_SAMPLE_PRODUCT` | Canonical product ID; required to enable remote |
| `CYBERGRID_SAMPLE_SPA_CLIENT` | Allowed real SPA client; required for Gateway |
| `CYBERGRID_SAMPLE_BROWSER_ORIGIN` | Exact allowed browser origin |

Native test server/issuer/client/resource defaults remain available; they do not imply a configured external Gateway or SPA. Configure the product and both actual clients in the coordinator's static product policy. Scopes are `openid offline_access profile email connect`. A later product move cannot silently migrate this host to another product.

The Web uses its own official Logto public SPA app and the same account/resource/product, with exact `<frontend-origin>/auth/callback`. Its public build variables are shown in `web/.env.example`. The frontend needs an origin reachable from the phone, normally fixed HTTPS. Quick Tunnel origin churn does not authorize callback or origin allowlist widening. No UI accepts tokens, addresses, arbitrary adapter code or ports.

```sh
npm --prefix examples/ram-only-workspace/web run dev
npm --prefix examples/ram-only-workspace/web run build
```

The default Web entry can log in and view the authorized device directory once its fixed configuration is supplied. Remote requests explicitly remain unavailable without an adapter. Page refresh loses RAM authorization and requires login again.

## Ownership and creation chain

```text
Desktop process
  ├─ Workspace RAM → LocalWorkspace → independently authorized local browser
  └─ official Logto RAM owner → correlated AuthProvider → Rust Client (off)
       └─ explicit enable → actual Gateway lease + independent bearer upstream
            → host TCP gate → actual provider endpoint
            → trusted policy + exact descriptor publication/confirmation
            → connectable; actual business authorization is still separate

SPA official RAM owner
  ├─ independent coordination-token context → JS directory/watch
  └─ fresh business-token closure → deliberately trusted adapter
       → controlled JS SDK wrapper → authenticated business DTOs/UTF-8 echo
```

| Module | Owns |
| --- | --- |
| `desktop/src/ram-owner.ts` | Official client, RAM grants, bounded reply escrow and serial refresh |
| `desktop/src/native-jwt.ts` | Pinned signed ID/access JWT checks, owner/client/resource continuity |
| `desktop/src/auth-ipc.ts` | Main-only correlated replies, uncertain reply delivery and generation barriers |
| `desktop/src-tauri/src/host.rs` | Explicit compound enable/disable, fixed product, late startup and cleanup |
| `desktop/src-tauri/src/native_close.rs` | One owned SDK close future and shared cleanup receipt |
| `desktop/src-tauri/src/bridge_publisher.rs` | Descriptor history, actual confirmation, renewal and local hard deadline |
| `desktop/src-tauri/src/gateway_host.rs` | Trusted lifecycle traits, endpoint sequence monitor and TCP gate |
| `local-workspace` | Independent local browser launch/session/CSRF and business HTTP/WS |
| `gateway-upstream` | Separate Gateway-only bearer HTTP/WS, never account/control APIs |
| `workspace-core` | Bounded synthetic RAM records and account/remote summary |
| `web/src/authorization.ts` | Official sole SPA RAM owner, popup callback and private fresh capability |
| `web/src/index.ts` | Directory UI, explicit key confirmation and controlled business operations |

No CredentialStore, refresh-grant import into Rust, Web Storage or keychain is used. A process kill loses all cloud authorization and private keys. “Exit local authorization” drains dispatched operations and clears local RAM; it does not claim Logto global SSO logout or confirmed cloud grant revocation.

## Actual external Gateway injection

A private licensed entry reuses the entire public Tauri chain:

```rust
pub fn run_with_factory(
    config: BuildConfig,
    factory: Option<Arc<dyn GatewayFactory>>,
);
```

The original `run()` delegates with defaults and `None`. IPC caller checks, navigation restrictions, token owner, window and exit handling have one implementation. Do not copy a second host into a fixture.

Trusted native assembly can use `run_with_factory_and_local_opener(config, factory, Arc<dyn LocalBrowserOpener>)` to select the browser for the initial LocalWorkspace open. The trait in `local_browser` receives the freshly minted URL and returns an owned `LocalBrowserOpenFuture` with the fixed `LocalBrowserOpenError` marker. This hook is called only by the existing main-window `local_open` command, after its caller check. Its dispatched future continues if the IPC waiter is cancelled. Errors return `BROWSER_OPEN_FAILED`, with no system fallback. The original `run()` and `run_with_factory` keep the system opener; OAuth always keeps its existing browser path. The trusted implementation must deliver the capability directly in process or over a controlled private pipe, never through URL IPC, argv, logs or files, and must not copy an already-open browser session.

`GatewayFactory::start(LaunchContext)` returns an owned `GatewayLease`. The private context contains fixed issuer/resource/SPA/owner/origins, a dedicated RAM upstream bearer and cancellation. Never serialize it, pass it in argv, log or persist it. The lease supplies:

```rust
fn ingress(&self) -> SocketAddr;
fn identity(&self) -> GatewayIdentity; // actual installation_id and handshake public_key
fn capabilities(&self) -> GatewayCapabilities; // immutable protocol and BridgeLimits
fn endpoint_updates(&self) -> watch::Receiver<GatewayEndpointState>;
fn bind_guarded_ingress<'a>(
    &'a mut self, address: SocketAddr, cancel: CancellationToken,
) -> BindFuture<'a>;
fn close(self: Box<Self>) -> CloseFuture;
```

`GatewayEndpointState` is `Pending`, `Available(GatewayEndpoint { revision, provider_id, url })`, `Unavailable { revision }` or `Closed { revision }`. Metadata contains no secret. Revisions are positive safe integers, increase on content/availability changes and cannot mutate at the same revision. The host subscribes before bind; the same monitor and observed history survive bind completion. `watch` may coalesce unobserved events, but an observed revision cannot roll back at handoff or later. Producer closure or invalid state seals ingress.

The lease's immutable protocol is `anywork.remote.v1`; declared limits must reflect the actual implementation. The pinned external implementation supports at most 65,535 frame bytes, 2 streams and 8 pending requests. The host validates these bounds and the SDK validates the complete descriptor and trusted endpoint policy.

The actual provider must target **only the host TCP gate**. The private wrapper owns its actual Gateway, cloudflared child and endpoint producer, publishes the parsed actual endpoint and reports process exit. Its consuming close cancels, waits and reaps those resources. A Quick URL rewritten to loopback is only a local protocol fixture, never evidence of public Cloudflare connectivity.

## Publication, renewal and close

The actual Host checks the fixed product, lease identity, trusted endpoint policy and active SDK scope. It saves bridge intent, enables the SDK, reads the complete directory and matches its own device/incarnation, exact descriptor, server publish revision and confirmed intent revision. Success means **connectable**, not business-authenticated.

Address revision belongs to the coordinator epoch/incarnation plus bridge ID. Key, installation, URL/provider, protocol or limits changes increase it. Expiry-only renewal preserves it. A new lease cannot clear history in the same incarnation; a confirmed real new incarnation can begin a new history. Any endpoint change currently takes the conservative path: close both entries and require explicit host reopen. There is no automatic in-place migration or background reopen.

The owner normally renews every **60 seconds** for approximately **180 seconds**, within the server's five-minute limit. Its monotonic hard deadline is bounded by the request start and trusted directory `sampledAt` (server time), with a safety margin. Confirmation must arrive before the previous local deadline; even a server-success response cannot revive an expired publication if the watchdog has not yet been scheduled. Old generation/identity/intent ACKs cannot restore readiness. Independent endpoint/deadline tasks do not depend on WebView rendering or remote JS polling. Publisher termination also seals the compound entries.

Automatic closure records its first fixed reason in `sample_view.error` in the same synchronous control fence that closes the gate. Startup timeout, endpoint change/unavailability/closure/watch loss, publication failure/expiry and publisher termination remain fail-closed. Publication errors are allowlisted fixed codes; external error text is never returned. Cleanup and explicit disable preserve that cause; a newly admitted enable clears it, and late tasks from an older generation cannot overwrite the new run. This diagnosis identifies the observed close branch, not the underlying network cause or a guarantee that a previous real-world closure had the same cause.

Disable, logout, exit or failure seals the host TCP gate/upstream and SDK Native gate before slow network cleanup. Local business continues. Bridge intent is withdrawn and the same owned Native cleanup receipt is awaited; unknown cloud cleanup stays pending. A late factory or bind result is still consumed and closed, never installed.

The SDK currently closes its local atomics/cancellation **before the first await inside `close()`**. The sample relies explicitly on that contract: it boxes one `Send + 'static` `disable_remote()` future, safely polls it once using a safe no-op waker without a host/control lock, then immediately gives that same pending future to the captured Tokio runtime. Tokio repolls with its real waker. Both Ready and Pending paths share one receipt; no dispatched future is discarded or duplicate close started. Host creation requires an active runtime handle; synchronous seal does not discover a runtime or panic if called from a later exit callback. A real Native mapped TCP test checks SDK gate state at seal return and stream termination; SDK changes to the first-await boundary must preserve this regression.

## Remote Web and adapter boundary

`web/src/index.ts` exports:

```ts
mountRemoteWorkspace(element, config, {
  createAdapter?: (authorization: BusinessAuthorization) =>
    TrustedBridgeAdapter<SampleRequest, SampleResponse, SampleStreamInput>
}): { dispose(): Promise<void> }
```

Use `deliverCallback()` at the private frontend entry before mounting the normal page. The callback path removes code/state from the address before relay. The login click creates a popup synchronously, before discovery/navigation. Exact origin, popup source, state and generation bind the callback to its original RAM owner; the official SDK completes PKCE and ID JWT verification. A blocked popup, lost opener, destroyed owner or interrupted callback fails safely and requires new login. No third-party callback analytics or persistent relay is used.

`BusinessAuthorization` exposes only `fresh({ signal }): Promise<string>` to the deliberately trusted adapter constructor. It obtains a fresh resource JWT through the same official owner's serial `clearAccessToken()`/`getAccessToken(resource)` path and verified subject. Coordination uses a separate context callback on that same owner. Cancelling a waiter never discards an already dispatched rotation; unknown token results poison the owner rather than replaying a consumed refresh grant. No raw official client or refresh grant escapes.

The adapter implements the existing JS SDK `TrustedBridgeAdapter`, obtains business authorization privately and returns only authenticated bounded operations. The sample exposes info, session list/create and poll DTOs, plus a bounded synthetic UTF-8 echo. It does not accept arbitrary paths/RequestInit or expose the underlying transport, terminal or protocol channel. The actual adapter must preserve split multi-byte UTF-8 boundaries, bound output queues and close on overflow. These types do not implement an external protocol.

The pinned external connector accepts only a Quick Tunnel root and derives `/remote/v1/ws`; the private adapter must validate the exact `quick-tunnel` WSS descriptor and path before deriving that root. CyberGrid's generic custom-provider support does not mean that this external connector supports custom providers, and Quick Tunnel is not a production availability promise.

Directory failure clears availability and closes the controlled wrapper. Foreground return closes stale business handles and reconfirms directory/authorization before explicit reconnection. First/changed public keys require confirmation; installation association never merges identity. The adapter call also checks the newly sampled key against the key the user confirmed. Writes run only after an explicit click; unknown write outcomes are not automatically replayed. Repeated dispose joins the same cleanup promise.

Every connection choice has its own monotonically fenced attempt, created before waiting for an older close. A newer choice, cancellation, unknown directory, foreground recheck or logout invalidates that attempt immediately, including before adapter dispatch. Superseded results remain owned and are closed; they cannot replace the current connection or update its UI. Already dispatched cleanup continues after its UI waiter is superseded.

Echo reads enforce a 4,096-byte chunk limit and fatal incremental UTF-8 decoding. Terminal read/write errors immediately remove stream availability and close its controlled stream and parent connection; normal EOF also relinquishes the stream, and a terminal SDK read rejection takes the same conservative close path. Stream cleanup has a one-second local budget before sealing its parent; the actual cleanup promise remains owned and the SDK bounds the parent cleanup wait. Old read/write completions cannot write errors or output into a newer connection. Opening another stream also fences an older pending open, so a late stream is closed rather than delivered.

## Local browser and Gateway upstream boundaries

The local listener binds `127.0.0.1` on a random port. Desktop opening creates a 256-bit RAM one-time launch capability, valid at most 30 seconds, in a fragment cleared before same-origin exchange. At most 16 launches and 16 one-hour sessions are retained. Protected local HTTP uses POST with exact Host/Origin and independent CSRF; instance-specific cookies are HttpOnly/SameSite=Strict/no Domain. WS requires cookie, exact Origin and first-frame CSRF. There is no CORS, forwarded-host trust, external account/control access or arbitrary localhost alias. Local trusted processes/same-origin scripts remain inside the local trust boundary.

The distinct Gateway listener checks exact loopback Host, a dedicated RAM bearer and rejects any browser Origin/cookie. HTTP uses `Authorization: Bearer`; WS uses the actual Gateway's first `{token}` frame within three seconds. It only implements `GET /api/info`, `GET/POST /api/sessions`, `GET /api/events/poll` and `/ws/terminal/{id}` synthetic echo. It is not a shell. Records are at most 1,024 bytes and the newest 64 are retained; synthetic WS messages are at most 4,096 bytes. Closing remote does not close the local listener or existing authorized local streams.

## Verify and evidence limits

Standalone local module checks:

```sh
cargo test --locked --manifest-path examples/ram-only-workspace/Cargo.toml --workspace
cargo clippy --locked --manifest-path examples/ram-only-workspace/Cargo.toml --workspace --all-targets -- -D warnings
cargo build --locked --manifest-path examples/ram-only-workspace/Cargo.toml --workspace
npm --prefix examples/ram-only-workspace/desktop test
npm --prefix examples/ram-only-workspace/desktop run build
npm --prefix examples/ram-only-workspace/web test
npm --prefix examples/ram-only-workspace/web run build
```

Complete repository acceptance additionally installs server dependencies and a real Redis executable, then explicitly includes repository integration tests:

```sh
npm ci --prefix server
REQUIRE_REDIS=1 cargo test --locked --manifest-path examples/ram-only-workspace/Cargo.toml --workspace -- --include-ignored
REQUIRE_REDIS=1 npm --prefix examples/ram-only-workspace/web run test:integration
```

Set `REDIS_TEST_SERVER` to an available Redis executable if it is not on PATH. Repository fixtures import the actual coordinator/Redis/JWK sources; a sample-only archive must not describe skipped repository tests as passed.

The Rust fixture checks real finite-v2 coordination, exact own publication, a real default 60-second renewal, actual Native mapped TCP data and synchronous close/EOF with local service preserved, endpoint loss without browser polling, persistent observed endpoint sequence and late-deadline rejection. Publication confirmation pins the SDK-confirmed revision before reading the directory and accepts a matching complete descriptor at that revision or later; an independent heartbeat advancing the revision after sampling does not invalidate that confirmation. A real default 20-second heartbeat regression and obsolete-revision, descriptor-change, intent-change and cancellation cases cover this boundary. Its external Gateway ingress is explicitly synthetic TCP, not Noise. Web tests use the official client with controlled ES384 OAuth/JWK responses for popup/callback and serial/unknown/cancelled authorization. The repository Web test combines that owner with actual finite-v2 Node/Redis/ES384 registration, JS console directory and controlled adapter invalidation; the adapter remains a lifecycle fixture, not cryptographic proof.

TCP gate tests retain the gate's own listener while proving new business admission is refused, then check existing/in-flight sockets close, its owner joins and the original local target remains usable. They do not infer ownership from an already released ephemeral port: another parallel fixture may legitimately reuse that address.

Actual external Noise/Cloudflare, real Logto cloud login, Android Chrome Wi-Fi/4G/switch/lock-screen return, 30-minute authorization cycling, 8-hour mixed load, 24-hour admission and deployment remain separate integration-010 release gates. Neither these fixtures nor SDK tests replace them. External source/dependencies and private test credentials/evidence are not bundled or redistributed; this README does not grant external AGPL embedding/distribution rights.
