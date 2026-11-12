# CyberGrid Rust SDK 0.3.0 (prepared for v0.8.0)

Version 0.3.0 prepares the explicit remote gate, context-aware authorization and finite-admission APIs. It is not yet a published download; v0.7.0 retains Rust 0.2.0. Use the complete repository archive for sample/Node/Redis integration prerequisites. Final integration-010 cloud, phone and long-run gates remain separate.

Native Rust device discovery and TCP port mapping for macOS and Android. Rust is the primary SDK for new development. Go implements the legacy protocol and cannot register against a coordinator that requires finite-v2 admission. The control plane manages Logto login, device metadata, heartbeats and signaling. The current source tries WebRTC, then direct iroh QUIC, then an authenticated Cloudflare Quick Tunnel. The CyberGrid server never proxies application bytes. iroh relays are disabled; Cloudflare carries only the final fallback connection, with endpoint TLS inside the WebSocket stream.

Project release v0.7.0 includes the iroh integration. The v0.6.0 downloadable SDK and Lab packages still use WebRTC followed by Quick Tunnel. Use the matching v0.7.0 coordinator and SDK archives to exercise the new three-stage policy; do not infer iroh support from an older release's component version.

This crate implements HTTP v1 with the `memory-v1` coordination extension and explicitly negotiates finite-v2 admission when advertised. Legacy mode retains its older coordination limits. It does not invoke the Go SDK or use a Go FFI bridge. Fixed cloudflared resources are bundled for macOS arm64 and Android arm64; no separate installation or runtime download is needed. Android hosts install the included native resource with their APK.

## Add to an application

For the downloaded source archive, extract it beside your application's directory. With `my-app/Cargo.toml` and `cybergrid-sdk-rust-0.3.0/Cargo.toml` in sibling directories, put this in `my-app/Cargo.toml`:

```toml
[dependencies]
cybergrid-sdk = { path = "../cybergrid-sdk-rust-0.3.0" }
tokio = { version = "1", features = ["full"] }
```

Cargo resolves `path` relative to the application's Cargo.toml. When using the repository checkout instead, an application directory directly beside `sdk-rust/` uses `path = "../sdk-rust"`; adjust the path for any deeper application directory. The Tauri test application in this repository uses `path = "../../../sdk-rust"` from `apps/tauri-test/src-tauri/Cargo.toml`.

An application ID is mandatory. Supply the exact, case-sensitive **Logto Native application client ID**. No test ID or default application is silently selected. Applications are managed in Logto.

```rust,no_run
use cybergrid_sdk::{Client, Config, CredentialStore, Port};

# async fn example() -> Result<(), Box<dyn std::error::Error>> {
let client = Client::new(Config {
    app_id: std::env::var("CYBERGRID_APP_ID")?,
    ..Config::default()
})?;
// Choose an explicit, app-private path; Android uses the application's files dir.
let store = CredentialStore::new("./private-device/credentials.json");
client.use_credential_store(store.clone()).await?;
client.set_auth_provider(std::sync::Arc::new(store.clone())).await;
// The host first completes official SDK browser sign-in and grant handoff.
if !store.exists() {
    return Err("Complete browser sign-in and credential handoff in the host first".into());
}
let tokens = store.refresh(&client).await?;
let session = client.login_with_access_token(&tokens.access_token, "My device").await?;
client.expose(Port::tcp("sync", 6789, "my-sync-protocol")).await?;
client.enable_remote().await?; // Explicit host action: publish intent and open inbound access.
let devices = client.devices().await?;
// Choose another device from devices; local mapping must bind loopback.
if let Some(remote) = devices.iter().find(|d| d.id != session.device_id && d.online) {
    let mapping = client.map(&remote.id, "sync", "127.0.0.1:0".parse()?).await?;
    // Connect application TCP code to mapping.addr. The listener remains active
    // until mapping.close() or client.close(). Old TCP streams end on reset;
    // a new TCP connection to the same addr uses fresh metadata and mTLS.
    println!("{} via {:?}", mapping.addr, mapping.current_transport().await);
    mapping.close().await;
}
client.close().await?;
# Ok(()) }
```

`Port::tcp(id, port, type)` describes an already-running local TCP service. **Behavior upgrade: inbound remote access now defaults to closed.** `expose()` saves local intent; it publishes only after the host explicitly calls `enable_remote()`. A client can discover devices and initiate an outgoing connection while its own inbound gate remains closed. Reverse streams on that outgoing connection also obey its inbound gate. IDs are application-defined UTF-8 strings (up to 64 bytes); type is an application-defined descriptive string. `Peer::dial(port_id)` provides an async byte stream if a local TCP listener is unnecessary. SDK payloads contain no application messaging or synchronization protocol.

## Login, persistence and cancellation

The current coordinator uses ordinary Native applications with Device Flow disabled. The host completes Authorization Code + PKCE with an official Logto SDK; CyberGrid Lab uses the JavaScript SDK on macOS and Android SDK on Android. Import the latest resource grant with `Client::import_browser_grant(&AuthConfig, OAuthTokens)`, using verified configuration from `auth_config()`. Only `OwnedPersisted` is ready for device login. `OwnedPending` transfers ownership to Rust but requires the retained Client/store to complete checkpoint or revocation; never continue registration or drop the pending owner. The producing SDK must stop token operations after either ownership acknowledgement. See the [host handoff contract](../docs/lab-browser-login.md).

The SDK verifies the server-advertised issuer, client ID, resource and same-origin HTTPS OAuth endpoints. Historical Device Flow methods remain for source compatibility, but refuse before requesting device authorization when the server declares `loginFlow: "authorization_code"`. Refresh rotation is saved atomically before device exchange. Tokens are private Rust values, have redacted Debug output, and are not serializable. `Session` serialization excludes its device session key.

`CredentialStore` is an optional Unix private-file implementation: explicit path, version 4 namespace and full OAuth endpoint/scope binding, 0600 regular files, no symlinks, cross-process flock and atomic replacement. Versions 2/3 (no automatic migration), another server/application/client ID, or changed issuer/resource/scope/trusted endpoints fails while preserving the file. The SDK never discovers or imports Go credential files. For Keychain/Keystore-backed persistence, use the public OAuth token methods and persist the latest refresh grant in the host's secure store instead.

Cancellation stops waiting to poll. First grant and every returned rotation are privately owned before validating resource fields or performing a second request. Bind the store with `use_credential_store(...).await?` before minting. A bound store checkpoints immediately; if storage fails, its private pending owner retains the latest material and the cross-process file-lock lease. Another process cannot refresh the stale file until checkpoint or revocation succeeds. `retry_pending_grant()` retries under the same authorization lock; `revoke_pending_grant()` can revoke the latest private grant after Client close even while storage remains unwritable. Keep the Client/store alive until one completes. A process crash between provider mint and local persistence cannot be made cross-system atomic.

Once a token poll is sent, the SDK completes any successful resource exchange and returns the latest rotated grant even if cancellation arrives. **Do not drop the entire completion or refresh future.** Retain its private owner and finish checkpoint or revocation for a canceled or stale UI generation. Logout revokes the refresh grant before removing the file; a revocation failure preserves it. Device-session close and OAuth grant logout are separate operations.

A Client has one explicit login/close lifecycle with automatic coordination recovery inside it. `set_auth_provider(Arc<dyn AuthProvider>)` supplies the host's single token owner. New providers implement `access_token_for(&Client, &AuthTokenRequest)`: the request pins issuer/client ID/resource/scopes, `min_validity` seconds, reason (`Connect`, `CoordinationRecovery`, `AuthorizationRejected`), `force_refresh`, SDK generation and a cancellation token. `Config.auth_min_validity` defaults to 60 and accepts 1..=3600 seconds. Honor freshness and force refresh in the host's official Logto SDK, while serializing any refresh rotation there. The legacy `access_token(&Client)` method remains source compatible through the default adapter, but cannot guarantee those freshness requirements. Provider errors become fixed `AuthorizationUnavailable`; do not put tokens in host diagnostics.

`login_with_auth_provider(name)` performs first login and recovery without constructing a `CredentialStore` or importing the refresh grant into Rust. The host keeps its official SDK and grant in RAM. This integration requires neither Web Storage nor a credential file; after process restart the host authorizes again. Keep the dispatched owner future alive until rotation/checkpoint completes, including when closing the Client. `close()`/`disable_remote()` wait for that owner and never log out Logto or discard a pending grant. The file-based handoff described above remains an explicit alternative, rather than a requirement for RAM-only integration.

```rust,no_run
use cybergrid_sdk::{AuthProvider, Client, Config, Port};
use std::sync::Arc;
// The host implements access_token_for with its official Logto SDK token owner.
async fn ram_only_device(provider: Arc<dyn AuthProvider>) -> cybergrid_sdk::Result<Arc<Client>> {
    let client = Client::new(Config { app_id: "actual-native-id".into(), ..Config::default() })?;
    client.set_auth_provider(provider).await;
    client.login_with_auth_provider("My device").await?;
    client.expose(Port::tcp("sync", 6789, "workspace-sync")).await?;
    // Invoke only after the user's explicit remote-access action.
    client.enable_remote().await?;
    Ok(client)
}
```

The first explicit `401 INVALID_IDENTITY` permits exactly one forced token request using the same registration ID. Other denials never trigger a refresh workaround. An uncertain exchange returns `ExchangeOutcomeUnknown` and retains its exact private token, request ID, revision, name and coordinator config. Call the same login method again to replay that operation; supplied replacement tokens/names are ignored while it is uncertain. Even a later 401 cannot turn that attempt into a fresh login. Recovery follows the same rule. Keep the same Client alive to resolve the result; do not mint new Clients to bypass registration fences. The sole domain-change exception is a fresh configuration read from the fixed trusted endpoint that passes the complete immutable OAuth binding checks and explicitly reports a new coordination epoch. Only then may the old epoch attempt retire and a new request/token be created. An exchange 401/409 or a failed/mismatched config read cannot establish that exception. This proves a new coordination domain, not physical deletion of an old deployment. A close with an unresolved exchange reports revocation pending because an unknown server session cannot be claimed deleted.

Recovery preserves the verified user/app, private registration key, local port intent and mapping listeners; old TCP/SDP/candidates never replay. A changed verified user is rejected. Manual token login without a provider emits `requires-login` on coordination loss. Epoch change stops old streams immediately; ordinary network failures can retain them only within the 65-second lease. Sessions use generation/incarnation and serial publication revisions; signals use UUID dedupe and a 15-second long poll. A confirmed exchange followed by an uncertain first publication keeps its known session and starts the single recovery supervisor. Either explicit login retry or background recovery uses the same verified user/key and a higher registration revision before changing certificates. If background recovery finishes first, the explicit retry acknowledges that ready session. A queued recovery wakeup cannot cancel a newly recovered runtime. Remote access remains closed until explicitly enabled. `SESSION_SUPERSEDED` stops recovery. A known rejected/expired attempt may start another fenced attempt; an uncertain exchange may not.

`watch_state()` immediately exposes the current `StateSnapshot` via `receiver.borrow()` and then coalesces updates through `receiver.changed().await`. `snapshot()` rereads it after a broadcast lag. Revisions increase monotonically; generation and epoch identify coordination restarts. Identity, coordination, publication and transport reachability are separate layers with fixed reason/retryability; business authorization defaults to `Unknown` and is set only by the host through `set_business_authorization`. A working mTLS stream does not grant workspace authorization. `confirmed_intent_revision` is absent for pending intent; `server_publish_revision` records a successful coordinator response. A late response cannot confirm a newer local intent. The snapshots contain no tokens, private keys, provider error text or tunnel addresses.

`disable_remote().await` closes the local gate and cancels existing and in-flight remote peers before awaiting cloud revocation. It returns `{local_closed: true, coordinator_revocation: Confirmed | Pending}` without exposing raw errors. `Confirmed` covers the exact known sessions owned by this Client in the currently trusted coordination epoch; it is not a promise to delete sessions from every historical deployment. A trusted new-epoch config can retire an old unknown domain, while retiring the old deployment remains an operator responsibility. Same-epoch unknown exchanges and unacknowledged known-session deletions remain `Pending`. Cloud failure leaves the device locally closed; retry disable/close to revoke a known session later. It ends this Client's login lifecycle. Local TCP services continue running, and `port_intents().await` returns the saved ports. To reopen, the host explicitly creates a fresh Client using the same in-memory Logto token owner, restores those intents, logs in and calls `enable_remote()`; old TCP streams and the old Client never resume.

Events include `coordination-ready`, `recovering`, `requires-login`, `mapping-disconnected`, `mapping-ready`, `provider-restarting`, `provider-failed`, `closed` with public generation/epoch/reason only. Mapping `current_transport()` reflects live state; the legacy `peer` and `transport()` describe its initial peer.

A Client has one login lifecycle. Close it explicitly and construct a fresh Client to log in again. A failed central close can be retried. Subscribe with `client.subscribe()` for public transport/error events; never forward OAuthTokens or private store contents to a WebView.

## Finite admission and trusted configuration

`auth_config()` validates the configuration from the fixed coordinator. If `registrationAdmission` is absent, the SDK uses the legacy register protocol. If present, it must be exactly `{version:2,challengeSeconds:120,ticketSeconds:86400}`; null, malformed or unknown required capabilities fail before registration. After finite-v2 has been verified, its removal within the same epoch is a configuration error; the cached binding and pending operation remain intact. A complete trusted new epoch is negotiated separately. The SDK obtains a challenge and mints a ticket before register, retaining all private request material in this Client's RAM. It never downgrades after a finite-v2 error. A static product can group several real Native client IDs while each token stays bound to its actual client. The canonical product is fixed for the lifetime of this Client.

Dropped/uncertain mint and register operations retain the original token, request IDs, challenge/ticket and revision. Retrying login on the same Client replays that operation exactly; supplying another name or token does not replace it. A same-epoch 401, failed config fetch, or local clock passage cannot prove that the old operation expired, and cannot authorize another domain. The public error remains `ExchangeOutcomeUnknown`; `watch_state()` distinguishes `AdmissionOutcomeUnknown` from a register outcome. Inspect fixed reasons without logging private request material.

Only a complete trusted config with a new epoch can retire an unknown old coordination scope. Within the same epoch, an explicit service `REGISTRATION_ADMISSION_EXPIRED` stops old replay but does not prove it never committed. The safe public reason becomes `AdmissionExpired` with automatic retry disabled; no ticket or provider text is exposed. Background recovery stops automatic replay at this definitive expiry and releases login serialization so the public login methods remain reachable; it must not replace this unknown operation. If the Client previously obtained a trusted `Session.user`, a later explicit host login may request fresh authorization, verifies that returned sessions still have that subject, and retains the bounded old-close-pending fact. If its very first result remained unknown, this Client has no verified subject with which to compare a fresh token: it remains Unknown/Pending. Close it, preserve that pending cleanup fact independently, have the host verify its official Logto owner, then explicitly construct a new Client. Do not automatically copy intents belonging to an unconfirmed owner. The SDK does not decode opaque tickets or unverified JWT payloads to invent identity continuity.

A known session reaching its absolute ticket deadline may recover through the single AuthProvider with `CoordinationRecovery` and `force_refresh=true`, keeping the same registrationKey/logical device ID and verified subject. Without that owner, or if authorization fails, the host must log in. This path cannot bypass an unknown old operation. Recovery does not turn on a disabled inbound gate. Old TCP streams close; mappings reconnect with fresh metadata rather than replaying business bytes.

OAuth ownership binds provider, issuer, client, resource, scope, loginFlow and every advertised OAuth endpoint. Coordination and verified product/endpoint policy metadata can change independently. `AuthConfig::same_client_binding(&other, explicit_native_client_id)` validates both configs and compares that full immutable OAuth binding plus the selected canonical product; it deliberately ignores mutable metadata. It is a comparison helper for configurations already obtained through the same trusted service, **not an authorization or provenance proof**. Policy removal or a product move closes the current Client instead of migrating it. Refresh-grant ownership uses the immutable OAuth binding so policy updates cannot strand the sole token owner.

## Complete directory and browser bridge intent

When `endpointPolicy` is advertised, `directory(cursor).await` obtains the complete bounded snapshot: version, product, epoch, policy version, opaque revision, server sampled time, reset/changed and all devices. `devices()` uses this validated snapshot as well. Retain the cursor only for the same authenticated scope; even `changed=false` returns a full snapshot and may include newer publication acknowledgements. A directory failure or unresolved policy change reports reachability Unknown; an old ready address is never returned as fresh. Legacy servers without this capability retain `/devices`; `directory()` explicitly reports unsupported rather than fabricating a snapshot.

`Device.bridges` contains host-declared browser bridge metadata. `DirectorySnapshot::devices_for_installation(id)` filters an already authorized snapshot by actual `bridge.installationId` for display association only. It does not merge device IDs, certificates, sessions or permissions. The SDK neither implements a browser bridge server nor treats a bridge URL as an authenticated SDK transport.

```rust,no_run
# async fn example(client: &cybergrid_sdk::Client, bridge: cybergrid_sdk::BrowserBridge) -> cybergrid_sdk::Result<()> {
client.publish_bridge(bridge).await?; // Save intent; gate remains under host control.
client.enable_remote().await?;
let snapshot = client.directory(None).await?;
for device in snapshot.devices_for_installation("my-installation") {
    println!("{}", device.name);
}
client.withdraw_bridge("my-bridge").await?;
# Ok(()) }
```

A `BrowserBridge` requires id, protocol, providerId, WSS URL, installationId, canonical base64url 32-byte publicKey, a positive safe-integer addressRevision, canonical millisecond UTC expiresAt, and limits (`maxFrameBytes` 1..65535, `maxStreams` 1..32, `maxPendingRequests` 1..128). Expiry must be within five minutes. `renew_bridge(id, expires_at)` changes expiry without decreasing it; changing any other address identity requires a larger addressRevision. Withdrawing retains its revision history in the current incarnation; at most four historical IDs are allowed. A new confirmed incarnation resets that history. `bridge_intents()` exposes saved nonsecret descriptors, including any expired local intent; heartbeat omits expired descriptors and never extends their deadline automatically.

A bridge publication requires the versioned capability. Saving intent with the local gate off does not publish it. Successful intent methods save the desired state even if a subsequent cloud publication fails; observe `confirmed_intent_revision` and the current validated directory/deadline before claiming availability. A general publication acknowledgement does not prove an expired bridge is usable. The whole device publication, including ports, bridges and iroh, is limited to 24 KiB. Disabling the Client closes its real peers and withdraws active publication, while preserved intent does not silently reopen anything.

The fixed public `endpointPolicy` includes a deterministic hash and a bounded list of exact HTTPS origins and paths. The SDK revalidates that hash and URL spelling without fetching those endpoints. Built-in Quick Tunnel rules remain fixed. Unknown providers, credentials, query/fragment, percent or backslash normalization, dot segments, explicit default ports, and mismatched authority/path are rejected. Native device URLs are HTTPS roots; business-port URLs can use a configured exact HTTPS path; bridges use their configured exact WSS path. They are distinct capabilities.

`update_device_tunnel_url(Some("https://device.example.test:8443"))` declares a host-operated ingress only after that exact origin and `/` are allowed by trusted policy. It must forward the SDK's fixed `/connect` to its local listener. Dialing derives **`wss://device.example.test:8443/connect`**, retaining the entire authority, including nondefault port or bracketed IPv6. `None` withdraws that explicit override (an enabled bundled provider may still supply its own Quick Tunnel). This never aliases a business HTTP port or browser bridge. The existing WebSocket transport still requires inner device mTLS and current same-owner/product authentication.

## Transport and provider configuration

Defaults: server `https://cybergrid.aixgg.com`, `direct_strategy: DirectStrategy::Sequential`, WebRTC direct timeout 12 seconds, `iroh_enabled: true`, `iroh_timeout: 8 seconds`, Cloudflare fallback timeout 35 seconds, signal long-poll 15 seconds (with a 1-second minimum spacing); Google and Cloudflare STUN for WebRTC; no interface filter. Only `stun:` URLs are accepted; TURN is disabled. `interface_name` can select the real network interface when testing. Shortening `direct_timeout` alone does not force Cloudflare: iroh can still connect. Set `iroh_enabled: false` as well when explicitly testing the final fallback.

Use `DirectStrategy::Parallel` to start both direct transports together. Only the first connection that passes inner mTLS and current-directory authentication can win; the other attempt is canceled and awaited before returning. Cloudflare is attempted only when both direct paths fail. Sequential direct budgets add; parallel direct budgets use their maximum. Per-attempt timeouts remain independent.

```rust,no_run
use cybergrid_sdk::{Config, DirectStrategy};
let config = Config {
    app_id: "your-native-client-id".into(),
    direct_strategy: DirectStrategy::Parallel,
    ..Config::default()
};
```

iroh is pinned to 1.4.1 with relay mode disabled and no default address lookup. The coordinator publishes an optional endpoint ID and up to eight numeric socket addresses in the authenticated device snapshot; no iroh keys are persisted and no relay URL is accepted. Missing peer metadata skips this stage for older peers. Each runtime generation owns a fresh endpoint, which closes with the generation. The iroh timeout includes its direct connection and inner authentication; mapping recovery includes all three transport budgets. Failed or canceled attempts do not replay application data. The transport event and peer label for a successful connection are `iroh-direct`.

Disabling relays also restricts iroh's NAT traversal and address discovery capabilities; the extra transport does not guarantee direct connectivity between arbitrary Wi-Fi and mobile networks. When `interface_name` is set, iroh is skipped because this integration cannot enforce the same interface restriction. It must not silently bypass the selected interface.

All three paths require endpoint TLS 1.3 mutual authentication, a single Ed25519 leaf certificate, device-ID CN, validity checks, real signature verification and SHA256 fingerprint matching against the current authenticated same-account/application directory. The QUIC endpoint ID is an additional transport identity, not a substitute for that authentication. Heartbeat lease and metadata invalidation close stale peers. Certificates and keys remain in memory. WebRTC uses a detached `cybergrid-v1` DataChannel; iroh carries the same authenticated byte protocol over a bidirectional QUIC stream; fallback uses binary WebSocket messages and the same subprotocol, with inner TLS above it. smux V1 streams preserve half-close and bound application frames to 32768 bytes.

To enable an inbound Quick Tunnel on macOS arm64, use `Config.cloudflared = Some(CloudflaredConfig::bundled(absolute_cache_base)?)`. The SDK securely extracts and revalidates its fixed resource with no PATH lookup or runtime download. `None` disables a local provider while still allowing remote tunnel dialing. `CloudflaredConfig::new(path)` is an explicit trusted external override; its bytes are not covered by the bundled guarantee. Provider lifecycle, readiness, bounded restart and cancellation semantics remain unchanged.

On Android arm64, run `python3 scripts/prepare-android-provider.py /absolute/host/jniLibs` from this crate root, enable Gradle `useLegacyPackaging` and no-strip for `libcloudflared.so`, then use `CloudflaredConfig::android_native(native_library_dir, app_cache)?`. It validates only the fixed APK-installed nativeLibraryDir resource, never writable app-home execution. Set `env_provider` to read the active LinkProperties DNS and app-cache TMPDIR before every spawn. The Android executable is not compiled into the Rust library a second time. Full offline resources, source pins, permissions, original license texts and packaging instructions are in [provider-assets/README.md](provider-assets/README.md).

Quick Tunnels are a temporary testing/development service, with changing hostnames and no uptime guarantee. Their current documented limit is 200 in-flight requests and they do not support SSE. This SDK uses WebSocket. See [the official Quick Tunnel limitations](https://developers.cloudflare.com/tunnel/get-started/quick-tunnels/).

## Advertise a business port's own tunnel URL

The host may create its own Quick Tunnel for a local business service and explicitly attach that HTTPS root URL to the port. The SDK does not automatically create a separate provider for each port. Its existing device-level tunnel remains the encrypted SDK WebSocket transport and must not be treated as a browser HTTP endpoint.

```rust,no_run
# async fn example(client: &cybergrid_sdk::Client) -> cybergrid_sdk::Result<()> {
let mut port = cybergrid_sdk::Port::tcp("api", 6789, "my-http-api");
port.tunnel_url = Some("https://example-name.trycloudflare.com".into());
client.expose(port).await?;
client.update_port_tunnel_url("api", Some("https://new-example-name.trycloudflare.com")).await?;
client.update_port_tunnel_url("api", None).await?; // Withdraw the URL; retain the port.
# Ok(()) }
```

Built-in Quick Tunnel URLs accept only canonical HTTPS single-label `trycloudflare.com` roots. A trusted advertised endpointPolicy may additionally allow exact custom origins and paths, including an explicit nondefault port. Before that policy is loaded, custom URLs fail explicitly. When remote access is enabled, a successful update publishes a new full snapshot; otherwise it only updates local intent. The coordinator bounds each device's complete serialized publication to 24 KiB, including port URLs and iroh metadata, and returns `DEVICE_METADATA_TOO_LARGE` before changing the snapshot if exceeded. If publication fails, local intent remains available for recovery; await a confirmed update before assuming remote clients have seen it. Recovery republishes the port URL with the port snapshot. An older client that omits it remains compatible.

The [read-only JS SDK](../sdk-js/README.md) can retrieve the explicitly declared port URL after the host logs into Logto. Business HTTP authentication and CORS remain the exposed service's responsibility; the URL itself is not an access credential. CyberGrid never fetches or proxies that service.

## Build and exercise

Previously validated release toolchain: Rust 1.97.1, macOS arm64, Android arm64 API 28 with NDK 27.2.12479018. iroh 1.4.1 raises the current source's declared Rust minimum to 1.91; that minimum and new platform behavior require their own checks and are not implied by the v0.6.0 release results. Cargo.lock is included; use `--locked`. Android requires its Rust target and NDK clang/linker environment. The SDK has no Windows implementation for the provided Unix credential store.

For the downloaded archive, run these commands from the directory containing `cybergrid-sdk-rust-0.3.0-source.tar.gz`. The archive has one top-level directory, `cybergrid-sdk-rust-0.3.0/`, which directly contains Cargo.toml, Cargo.lock, src/, examples/ and scripts/:

```sh
tar -xzf cybergrid-sdk-rust-0.3.0-source.tar.gz
cd cybergrid-sdk-rust-0.3.0
cargo test --locked --lib
cargo clippy --locked --all-targets -- -D warnings
cargo build --locked --example lab
```

From that same extracted crate directory, set `CYBERGRID_APP_ID` to your actual Logto Native client ID, then exercise the lab. Replace the absolute credential/provider paths and remote device ID with your own:

```sh
cargo run --locked --example lab -- login --app "$CYBERGRID_APP_ID" --credentials /absolute/private/credentials.json
cargo run --locked --example lab -- host --app "$CYBERGRID_APP_ID" --credentials /absolute/private/credentials.json
cargo run --locked --example lab -- probe --app "$CYBERGRID_APP_ID" --credentials /absolute/other-device/credentials.json --peer REMOTE_DEVICE_ID --bytes 65536
```

For a repository checkout, run the following from the **CyberGrid repository root** instead. The `--manifest-path` prefix selects the same crate without changing directory:

```sh
# Real repository integration tests need server npm dependencies and redis-server.
# Set REDIS_TEST_SERVER to its absolute path when it is not on PATH.
REQUIRE_REDIS=1 cargo test --locked --manifest-path sdk-rust/Cargo.toml --lib -- --include-ignored --test-threads=1
cargo clippy --locked --manifest-path sdk-rust/Cargo.toml --all-targets -- -D warnings
cargo build --locked --manifest-path sdk-rust/Cargo.toml --example lab
cargo run --locked --manifest-path sdk-rust/Cargo.toml --example lab -- login --app "$CYBERGRID_APP_ID" --credentials /absolute/private/credentials.json
cargo run --locked --manifest-path sdk-rust/Cargo.toml --example lab -- host --app "$CYBERGRID_APP_ID" --credentials /absolute/private/credentials.json
cargo run --locked --manifest-path sdk-rust/Cargo.toml --example lab -- probe --app "$CYBERGRID_APP_ID" --credentials /absolute/other-device/credentials.json --peer REMOTE_DEVICE_ID --bytes 65536
```

The standalone Rust archive runs its local unit/transport tests without Node. Tests marked `requires repository Node dependencies and a real Redis binary` are explicitly ignored by default because their fixture imports the repository coordinator. The repository acceptance/CI command above includes them: run `npm ci` in `server/` first and install Redis (or set `REDIS_TEST_SERVER`). They start isolated loopback Redis with temporary AOF data and local ES384 JWK handlers, never a real Logto Cloud user or production service. The fixture's expired signed claims exercise actual Redis TIME rejection without representing a 24-hour lifecycle run. A modeled known-client deadline separately tests recovery ownership; production duration and Wi-Fi/4G acceptance remain separate checks.

Authorize each physical device independently. Do not copy refresh credentials between Mac and Android. The lab also accepts an explicit HTTP loopback harness plus `--access-token-file` for local protocol fixtures; it never supplies a production identity fallback. `lab version` prints the Rust crate version. `--stun none` is for controlled local testing only.

Source and test binaries are distributed through [GitHub Releases](https://github.com/ywateru/CyberGrid/releases). The repository Actions bind each release manifest to its source commit and include SHA256SUMS, fixed provider resources and original notices. Build directories, local authorization and test records are excluded. See the repository release documentation for the CI packaging entry point.

Third-party declarations are in [THIRD_PARTY_NOTICES.md](THIRD_PARTY_NOTICES.md), with license/copyright/notice texts and source pins in [licenses/README.md](licenses/README.md). Original text files retain their bytes. For enum-assoc 1.4.2, whose exact published source declares `MIT OR Apache-2.0` but omits a license text, the collection explicitly selects Apache-2.0 and includes the canonical standard text plus the exact upstream declaration; it does not present that text as an upstream original. Dependencies are fetched separately by Cargo under their own licenses. The original CyberGrid code retains its existing rights notice; publishing source does not add a separate open-source license.
