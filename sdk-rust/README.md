# CyberGrid Rust SDK 0.2.0

Native Rust device discovery and TCP port mapping for macOS and Android. The control plane manages Logto login, device metadata, heartbeats and signaling. Application bytes pass between endpoints over WebRTC first, then an authenticated Cloudflare Quick Tunnel when direct connection fails. The CyberGrid server never proxies those bytes. Cloudflare carries the fallback connection; endpoint TLS remains inside the WebSocket stream.

This crate implements HTTP v1 with the `memory-v1` coordination extension and interoperates with Go SDK 0.4. It does not invoke the Go SDK or use a Go FFI bridge. Fixed cloudflared resources are bundled for macOS arm64 and Android arm64; no separate installation or runtime download is needed. Android hosts install the included native resource with their APK.

## Add to an application

For the downloaded source archive, extract it beside your application's directory. With `my-app/Cargo.toml` and `cybergrid-sdk-rust-0.2.0/Cargo.toml` in sibling directories, put this in `my-app/Cargo.toml`:

```toml
[dependencies]
cybergrid-sdk = { path = "../cybergrid-sdk-rust-0.2.0" }
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

`Port::tcp(id, port, type)` exposes an already-running local TCP service. IDs are application-defined UTF-8 strings (up to 64 bytes); type is an application-defined descriptive string. `Peer::dial(port_id)` provides an async byte stream if a local TCP listener is unnecessary. SDK payloads contain no application messaging or synchronization protocol.

## Login, persistence and cancellation

The current coordinator uses ordinary Native applications with Device Flow disabled. The host completes Authorization Code + PKCE with an official Logto SDK; CyberGrid Lab uses the JavaScript SDK on macOS and Android SDK on Android. Import the latest resource grant with `Client::import_browser_grant(&AuthConfig, OAuthTokens)`, using verified configuration from `auth_config()`. Only `OwnedPersisted` is ready for device login. `OwnedPending` transfers ownership to Rust but requires the retained Client/store to complete checkpoint or revocation; never continue registration or drop the pending owner. The producing SDK must stop token operations after either ownership acknowledgement. See the [host handoff contract](../docs/lab-browser-login.md).

The SDK verifies the server-advertised issuer, client ID, resource and same-origin HTTPS OAuth endpoints. Historical Device Flow methods remain for source compatibility, but refuse before requesting device authorization when the server declares `loginFlow: "authorization_code"`. Refresh rotation is saved atomically before device exchange. Tokens are private Rust values, have redacted Debug output, and are not serializable. `Session` serialization excludes its device session key.

`CredentialStore` is an optional Unix private-file implementation: explicit path, version 4 namespace and full OAuth endpoint/scope binding, 0600 regular files, no symlinks, cross-process flock and atomic replacement. Versions 2/3 (no automatic migration), another server/application/client ID, or changed issuer/resource/scope/trusted endpoints fails while preserving the file. The SDK never discovers or imports Go credential files. For Keychain/Keystore-backed persistence, use the public OAuth token methods and persist the latest refresh grant in the host's secure store instead.

Cancellation stops waiting to poll. First grant and every returned rotation are privately owned before validating resource fields or performing a second request. Bind the store with `use_credential_store(...).await?` before minting. A bound store checkpoints immediately; if storage fails, its private pending owner retains the latest material and the cross-process file-lock lease. Another process cannot refresh the stale file until checkpoint or revocation succeeds. `retry_pending_grant()` retries under the same authorization lock; `revoke_pending_grant()` can revoke the latest private grant after Client close even while storage remains unwritable. Keep the Client/store alive until one completes. A process crash between provider mint and local persistence cannot be made cross-system atomic.

Once a token poll is sent, the SDK completes any successful resource exchange and returns the latest rotated grant even if cancellation arrives. **Do not drop the entire completion or refresh future.** Retain its private owner and finish checkpoint or revocation for a canceled or stale UI generation. Logout revokes the refresh grant before removing the file; a revocation failure preserves it. Device-session close and OAuth grant logout are separate operations.

A Client has one explicit login/close lifecycle with automatic coordination recovery inside it. `set_auth_provider(Arc<dyn AuthProvider>)` enables single-flight recovery; the provider receives only the Client and returns a resource access token. The provided CredentialStore implements it and serializes refresh/checkpoint operations. Manual token login without a provider emits `requires-login` on coordination loss. Recovery preserves verified user/app, private registration key, local ports and mapping listeners; old TCP/SDP/candidates never replay. Epoch change stops old streams immediately, ordinary network failures may retain them only within the 65-second lease. Sessions use generation/incarnation and serial publication revisions; signals use UUID dedupe and a 15-second long poll. An uncertain first snapshot retires its registration attempt with a higher registrationRevision before creating a new certificate. `SESSION_SUPERSEDED` stops recovery; a retired idempotency cache (`REGISTRATION_ATTEMPT_EXPIRED`) starts a new fenced attempt.

Events include `coordination-ready`, `recovering`, `requires-login`, `mapping-disconnected`, `mapping-ready`, `provider-restarting`, `provider-failed`, `closed` with public generation/epoch/reason only. Mapping `current_transport()` reflects live state; the legacy `peer` and `transport()` describe its initial peer.

A Client has one login lifecycle. Close it explicitly and construct a fresh Client to log in again. A failed central close can be retried. Subscribe with `client.subscribe()` for public transport/error events; never forward OAuthTokens or private store contents to a WebView.

## Transport and provider configuration

Defaults: server `https://cybergrid.aixgg.com`, direct timeout 12 seconds, fallback timeout 35 seconds, signal long-poll 15 seconds (with a 1-second minimum spacing); Google and Cloudflare STUN; no interface filter. Only `stun:` URLs are accepted; TURN is disabled. `interface_name` can select the real network interface when testing. Direct is always attempted first. Shortening `direct_timeout` is an explicit fallback test setting, not a deployment default.

Both paths require endpoint TLS 1.3 mutual authentication, a single Ed25519 leaf certificate, device-ID CN, validity checks, real signature verification and SHA256 fingerprint matching against the current authenticated same-account/application directory. Heartbeat lease and metadata invalidation close stale peers. Certificates and keys remain in memory. WebRTC uses a detached `cybergrid-v1` DataChannel; fallback uses binary WebSocket messages and the same subprotocol, with inner TLS above it. smux V1 streams preserve half-close and bound application frames to 32768 bytes.

To enable an inbound Quick Tunnel on macOS arm64, use `Config.cloudflared = Some(CloudflaredConfig::bundled(absolute_cache_base)?)`. The SDK securely extracts and revalidates its fixed resource with no PATH lookup or runtime download. `None` disables a local provider while still allowing remote tunnel dialing. `CloudflaredConfig::new(path)` is an explicit trusted external override; its bytes are not covered by the bundled guarantee. Provider lifecycle, readiness, bounded restart and cancellation semantics remain unchanged.

On Android arm64, run `python3 scripts/prepare-android-provider.py /absolute/host/jniLibs` from this crate root, enable Gradle `useLegacyPackaging` and no-strip for `libcloudflared.so`, then use `CloudflaredConfig::android_native(native_library_dir, app_cache)?`. It validates only the fixed APK-installed nativeLibraryDir resource, never writable app-home execution. Set `env_provider` to read the active LinkProperties DNS and app-cache TMPDIR before every spawn. The Android executable is not compiled into the Rust library a second time. Full offline resources, source pins, permissions, original license texts and packaging instructions are in [provider-assets/README.md](provider-assets/README.md).

Quick Tunnels are a temporary testing/development service, with changing hostnames and no uptime guarantee. Their current documented limit is 200 in-flight requests and they do not support SSE. This SDK uses WebSocket. See [the official Quick Tunnel limitations](https://developers.cloudflare.com/tunnel/get-started/quick-tunnels/).

## Build and exercise

Validated toolchain: Rust 1.97.1, macOS arm64, Android arm64 API 28 with NDK 27.2.12479018. The declared Rust minimum is 1.88; that older toolchain has not been independently validated. Cargo.lock is included; use `--locked`. Android requires its Rust target and NDK clang/linker environment. The SDK has no Windows implementation for the provided Unix credential store.

For the downloaded archive, run these commands from the directory containing `cybergrid-sdk-rust-0.2.0.tar.gz`. The archive has one top-level directory, `cybergrid-sdk-rust-0.2.0/`, which directly contains Cargo.toml, Cargo.lock, src/, examples/ and scripts/:

```sh
tar -xzf cybergrid-sdk-rust-0.2.0.tar.gz
cd cybergrid-sdk-rust-0.2.0
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
cargo test --locked --manifest-path sdk-rust/Cargo.toml --lib
cargo clippy --locked --manifest-path sdk-rust/Cargo.toml --all-targets -- -D warnings
cargo build --locked --manifest-path sdk-rust/Cargo.toml --example lab
cargo run --locked --manifest-path sdk-rust/Cargo.toml --example lab -- login --app "$CYBERGRID_APP_ID" --credentials /absolute/private/credentials.json
cargo run --locked --manifest-path sdk-rust/Cargo.toml --example lab -- host --app "$CYBERGRID_APP_ID" --credentials /absolute/private/credentials.json
cargo run --locked --manifest-path sdk-rust/Cargo.toml --example lab -- probe --app "$CYBERGRID_APP_ID" --credentials /absolute/other-device/credentials.json --peer REMOTE_DEVICE_ID --bytes 65536
```

Authorize each physical device independently. Do not copy refresh credentials between Mac and Android. The lab also accepts an explicit HTTP loopback harness plus `--access-token-file` for local protocol fixtures; it never supplies a production identity fallback. `lab version` prints the Rust crate version. `--stun none` is for controlled local testing only.

Source and test binaries are distributed through [GitHub Releases](https://github.com/ywateru/CyberGrid/releases). The repository Actions bind each release manifest to its source commit and include SHA256SUMS, fixed provider resources and original notices. Build directories, local authorization and test records are excluded. See the repository release documentation for the CI packaging entry point.

Third-party declarations are in [THIRD_PARTY_NOTICES.md](THIRD_PARTY_NOTICES.md), with verbatim license/copyright/notice texts and source pins in [licenses/README.md](licenses/README.md). Dependencies are fetched separately by Cargo under their own licenses. The original CyberGrid code retains its existing rights notice; publishing source does not add a separate open-source license.
