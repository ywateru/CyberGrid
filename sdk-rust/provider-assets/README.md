# Bundled Cloudflared resources

These fixed offline inputs are part of the CyberGrid SDK distribution. Cloudflared runs as a separate client-side process; it only receives the SDK's private loopback WebSocket origin, whose byte stream has end-to-end mutual TLS. No account token, session key or business port is passed to the provider.

`manifest.json` binds both supported targets to the same canonical resources in `tools/cloudflared/`: macOS arm64 uses the official 2026.9.0 release binary; Android arm64/API28+ uses the previously verified 2026.9.0 source commit and explicit-DNS patch. The official macOS build reports `vcs.modified=true`; CyberGrid preserves the official bytes without changing them. Exact archive/raw sizes, SHA256, source and build information are recorded rather than inferring a clean upstream checkout. `LICENSE` and `THIRD_PARTY_NOTICES.md` reproduce the provider and union of linked module/runtime notices. `build-modules-*.txt` distinguish each binary's actual dependency pins.

macOS SDK binaries embed the target resource. The Go zero-value `Cloudflared{}` selects it; Rust uses `CloudflaredConfig::bundled(absolute_cache_base)?`. A dedicated 0700/current-user cache below the supplied base stores the pinned file and fixed-inode 0600 lock. The SDK checks SHA, size, regular-file/owner/mode, refuses symlinks, uses exclusive temporary output and atomic publication, and revalidates on each provider start. Existing shared OS/Tauri cache ancestors may be 0755; dedicated provider directories must be 0700. No PATH search, runtime download or auto-update occurs. An unsafe or damaged cache yields an error; no external fallback is attempted.

Explicit Go `Cloudflared{Binary: path}` and Rust `CloudflaredConfig::new(path)` remain trusted external overrides. Such a selection is not covered by bundled hash/install-directory guarantees. Other custom TunnelProvider implementations remain available.

## Android application packaging (offline)

From the extracted Go or Rust SDK root:

```sh
python3 scripts/prepare-android-provider.py /absolute/host/app/src/main/jniLibs
```

This verifies and copies exactly one `arm64-v8a/libcloudflared.so` from the SDK's bundled resources. Do not also embed the Android executable in the compiled Go/Rust library. If both SDKs appear in one application, choose one packaging output; their provider bytes are identical, so the APK must still contain only one resource. Configure the host's Gradle packaging:

```kotlin
packaging {
    jniLibs {
        useLegacyPackaging = true
        keepDebugSymbols.add("**/libcloudflared.so")
    }
}
```

The Android installer must extract this resource to `ApplicationInfo.nativeLibraryDir`. Only the fixed basename `libcloudflared.so` in an installed, non-app-owned/non-writable native directory is accepted, with exact size/SHA and executable/regular-file checks. Missing files, changed bytes, writable data paths, traversal and unsupported ABI fail explicitly. SDK default Android selection never extracts to writable files/cache or searches PATH. `targetSdk>=29` applications cannot execute files in writable app home; a shell command under `/data/local/tmp` is a separate harness and does not establish App compatibility.

Go hosts set `Cloudflared{NativeLibraryDir: nativeDir, TempDir: appCache, EnvProvider: currentEnvironment}`; Rust hosts use `CloudflaredConfig::android_native(nativeDir, appCache)?`, then set `env_provider`. Before **every** spawn the host callback reads the active `ConnectivityManager`/`LinkProperties.dnsServers`, formats its literal IP:53 (IPv6 as `[IP]:53`) in `CLOUDFLARED_DNS_SERVER`, and provides app-private `TMPDIR`. Never retain an old DNS across network changes or silently pick a public resolver. Environment lookup failures are explicit. Provider environment is cleared and populated only from the SDK's non-secret whitelist.

## Reproduction and offline use

All provider bytes needed by the supported SDK platforms are already in the source archive. Standard Go/Cargo dependencies still follow each language's lock/cache workflow; with those dependencies cached, `go build`/`cargo build --locked --offline` require no provider network fetch. Rust's Android build does not include the Android executable in its library; the resource is for the host's installation packaging.

For the Android source rebuild, run `bash provider-assets/rebuild/build-android.sh` with the documented pinned Go/NDK27.2/API28 toolchain. It clones the fixed upstream tag, checks its exact commit, applies the recorded DNS patch, verifies upstream module checksums and emits into `provider-assets/rebuild/bin/`; it does not overwrite the distributed fixed resource. Compare the rebuilt raw SHA to `manifest.json` before adopting anything. macOS reproduction uses the exact official release archive URL and separate archive/raw hashes in `manifest.json`, preserving the original upstream artifact. An upgrade requires a new resource manifest and full target verification; no runtime updater is enabled.

Quick Tunnels need Internet access and are intended for testing/development, with no uptime guarantee. Offline `--version` proves executability only. Official references:

- https://developers.cloudflare.com/tunnel/get-started/quick-tunnels/
- https://github.com/cloudflare/cloudflared/releases/tag/2026.9.0
- https://developer.android.com/about/versions/10/behavior-changes-10
- https://developer.android.com/guide/topics/manifest/application-element#extractNativeLibs
