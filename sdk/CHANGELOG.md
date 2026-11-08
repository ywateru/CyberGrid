# Changelog

## 0.4.0 — 2026-11-08 (local release candidate; publication pending)

- Bundle pinned cloudflared resources for macOS arm64 and Android arm64/API28+.
  Mac default extracts securely without PATH/downloads; Android hosts package one
  verified native resource. Include original provider/module/runtime notices.
- Negotiate memory-v1 with epoch, stable private registration identity, ordered
  registration attempts, incarnation-bound signals and serialized full snapshots.
- Recover automatically only with an explicit AuthProvider, preserving service
  declarations and loopback mapping listeners; new TCP connections reconnect
  after recovery. Old application streams disconnect without payload replay.
- Keep TLS identity and publication sequence when registration/first heartbeat
  responses are lost; do not reset a pin within the same incarnation.
- Checkpoint every minted/rotated grant before another fallible OAuth stage.
  Retain private pending grants on cancellation/resource/disk failures and keep
  the CLI file lease until a pending rotation is saved or revoked.
- Upgrade persistent credentials to version4 with complete OAuth endpoint/scope
  binding across processes. Refuse old v3/v2 bytes before refresh or overwrite.
- Preserve historical 0.3 archives. New local macOS/Android/source packages and
  notices do not imply public cloud deployment or WAN/device verification.

## 0.3.0 — 2026-11-07

- Use the real, case-sensitive Logto Native client ID as Config.AppID, with no
  default business slug, application aliases, or global Native-client fallback.
- Fetch public auth configuration with an encoded appId query and require its
  clientId to match. Require exchange purpose=device and the matching appId.
- Make Apps/CLI apps authenticated read-only Logto directory access. Return only
  id/name/type/isDeviceFlow; remove Enabled and local application catalog semantics.
- Require -app or CYBERGRID_APP_ID for authenticated CLI commands; no automatic
  .env loading. Persist version 3 grants bound to appId and clientId. Reject v2
  and cross-app files before refresh without overwriting or deleting them.
- Revoke old grants before API cutover using 0.2 logout, or ask the Logto
  maintainer after cutover. Local file deletion is not provider revocation.
  Version 3 logout preserves credentials on validation or revocation failure.
- Retain opaque-device-token to resource-token refresh, refresh rotation, file
  locking, mTLS, WebRTC, Cloudflare fallback, and existing mapping lifecycles.
- Rebuild macOS/Android binaries and source archives with updated notices,
  manifests and SHA256 hashes. Historical releases do not verify this contract.

## 0.2.0 — 2026-11-07

- Replace SDK account-credential login with Logto Cloud hosted OAuth Device
  Authorization Flow, using a public Native client without a client secret.
- Add `AuthConfig`, `BeginDeviceAuthorization`, `CompleteDeviceAuthorization`,
  `RefreshAccessToken`, `LoginWithAccessToken`, and `RevokeRefreshToken`.
- After the device grant, explicitly refresh with the API resource to obtain its
  access token; return the latest rotated refresh token. Never exchange the
  initial opaque OIDC token or an ID token.
- Exchange a resource access token for the existing CyberGrid app/device session.
  Legacy `Register` and `Login` return explicit unsupported errors and never
  contact account-credential endpoints.
- Add CLI `login`, `logout`, and `version`. Each device obtains its own grant;
  version 2 private credentials bind server, issuer, client ID, and resource.
  Refresh rotation uses file locking and atomic 0600 writes.
- Obey device-code pending, slow_down, denial, expiration, and cancellation.
  Validate HTTPS OAuth endpoints against the issuer origin, refuse configuration
  changes during a client lifecycle, and keep credentials out of diagnostic logs.
- Keep WebRTC, Cloudflare fallback, mTLS fingerprint authentication, stream FIN,
  session shutdown, and mapping behavior. New authentication requires new
  independent production verification; old releases' evidence remains historical.
- Deliver source, macOS arm64 CLI, Android arm64 API28 CLI, hashes, build metadata,
  and third-party license notices. This release does not include an APK/AAR/GUI.
