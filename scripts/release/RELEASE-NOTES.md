CyberGrid project release. The independently versioned components are Go SDK/CLI **0.4.0**, Rust SDK **0.2.0**, and CyberGrid Lab **0.2.0**.

- Native macOS arm64 CLI and Tauri test app; Android arm64 API28+ CLI and debug-test APK with the explicit ADB control feature.
- Fixed, offline cloudflared resources. macOS uses the SDK-bundled provider; Android uses one OS-installed native-library resource. No runtime provider download.
- Source archives and original license/notice texts; `SHA256SUMS` and `release-manifest.json` bind every asset to this Git commit and CI run.
- The macOS app is not notarized. The APK uses an ephemeral CI debug signing key; an app signed with another key cannot be upgraded in place. Preserve or revoke existing credentials through their original app before choosing an uninstall/reinstall.
- These jobs perform real packaging builds. They do not rerun WAN/device/OAuth acceptance tests or claim additional runtime validation.

Original CyberGrid code retains all rights reserved. Publishing source does not grant an additional open-source license. Third-party licenses remain separately effective; full texts accompany the artifacts.
