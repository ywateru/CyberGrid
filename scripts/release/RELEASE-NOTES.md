CyberGrid v0.6.0 replaces Lab's device-code sign-in with system-browser authorization code + PKCE. Deploy this coordinator and upgrade Lab together: the coordinator now accepts ordinary Logto Native applications with Device Flow disabled, so the Lab 0.2 and CLI device-code login from v0.5.1 cannot sign in to the new service.

The independently versioned components are Go SDK/CLI **0.4.0**, Rust SDK **0.2.0**, and CyberGrid Lab **0.3.0**. Use the source archives in this project release for the updated SDK code; the historical `sdk/v0.4.0` tag remains unchanged. The CLI and Rust CLI example retain legacy Device Flow entry points for source compatibility, but do not implement first-time browser login against this coordinator. Use Lab or integrate an official Logto SDK in your host application.

- Lab uses system-browser sign-in with Device Flow disabled: official Logto Android SDK on Android and JavaScript SDK in the desktop Tauri frontend. Public test server and Native application settings are preset; account credentials are not bundled.
- Both platforms hand off the grant to the Rust credential owner for later refresh, resume, and revocation. Android ADB control adds `web-login`; `login` is an alias for the same browser flow. Public connection arguments are optional. Applications are managed only in Logto; their actual, case-sensitive Native client ID remains the application isolation boundary.
- Native macOS arm64 CLI and Tauri test app; Android arm64 API28+ CLI and debug-test APK with the explicit ADB control feature.
- Fixed, offline cloudflared resources. macOS uses the SDK-bundled provider; Android uses one OS-installed native-library resource. No runtime provider download.
- Source archives and original license/notice texts; `SHA256SUMS` and `release-manifest.json` bind every asset to this Git commit and CI run.
- The macOS app is not notarized. The APK uses an ephemeral CI debug signing key; an app signed with another key cannot be upgraded in place. Preserve or revoke existing credentials through their original app before choosing an uninstall/reinstall.
- These jobs perform real packaging builds. They do not rerun WAN/device/OAuth acceptance tests or claim additional runtime validation.

Original CyberGrid code retains all rights reserved. Publishing source does not grant an additional open-source license. Third-party licenses remain separately effective; full texts accompany the artifacts.
