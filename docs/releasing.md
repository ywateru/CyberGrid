# Building and publishing a release

CyberGrid uses a project release tag, target `v0.8.0` (prepared, not yet published), while the components retain their own versions: Go SDK/CLI 0.4.0, Rust SDK 0.3.0, CyberGrid Lab 0.4.0, JS SDK 0.2.0, Node server 0.2.0 and RAM-only workspace 0.1.0. Publishing source does not grant an additional open-source license. See the root and component `LICENSE` files; third-party grants remain separately effective.

The v0.6.0 coordinator and Lab use ordinary Logto Native applications with Device Flow disabled. Deploy the matching coordinator with the client release. Historical device-code CLI login does not work with this coordinator; it remains only for older compatible services. Existing releases are immutable.

Project release v0.7.0 adds iroh direct connections, configurable WebRTC/iroh parallel attempts, per-port Quick Tunnel declarations, and the read-only JS SDK. These features are absent from the existing v0.6.0 downloads. Use the matching v0.7.0 coordinator and archives. Reusing a component version does not update an existing project release.

## Workflow

The `Build and publish native SDKs` workflow in `.github/workflows/release.yml` runs for project tags matching `v*`, or through `workflow_dispatch` with an explicit project release tag. The Go submodule tag `sdk/v0.4.0` does not match that trigger, so it does not start a duplicate project release.

The workflow has three packaging jobs plus a required reusable repository-integration gate:

- **sources**, on Linux: build the website and typecheck the Node service; build and test the JS SDK, including the legacy HTTP integration and the real Redis/dual-handler bridge-directory integration; verify both fixed provider resources; then package committed Go/Rust/JS/App source, built JS ESM/types, and license texts with recorded provenance.
- **macos**, on a native macOS arm64 runner: fixed Node 22.23.4, Go 1.27.0 and Rust 1.97.1; build the Go CLI and Tauri 2 debug-test app.
- **android**, on Linux: the same language toolchains, JDK 17, Android SDK Platform 37.0, Build-Tools 36.0.0 and NDK 27.2.12479018; build the Go arm64 CLI with the API28 NDK compiler and bionic DNS, plus the arm64 API28+ Tauri debug-test APK with `adb-test-control` enabled.

Actions are referenced by full commit hashes. npm uses `npm ci`; Cargo and Go dependencies have committed lock/checksum files. The fixed platform provider bytes are already in the repository. `scripts/prepare-android-provider.sh` copies the verified resource into the app's generated `jniLibs` directory; the APK contains one `libcloudflared.so`. No cloudflared binary is downloaded at runtime.

The reusable `.github/workflows/integration.yml` gate installs real Redis and Node dependencies on macOS, builds the actual service/ESM SDK, runs `cargo test --locked --manifest-path sdk-rust/Cargo.toml -- --include-ignored`, then tests/builds the RAM-only modules and official Logto owner. The Web module is required: the same gate installs its locked dependencies after desktop dependencies and runs `test`, `test:integration` (real Node/Redis/JWK), and build. Sample Cargo commands explicitly use `--workspace -- --include-ignored`; the virtual workspace already defaulted to all members, so the explicit flag is an audit clarification. The browser SDK workflow runs its full unit suite and both HTTP integrations with `REQUIRE_REDIS=1`. These controlled local tests do not fetch private AnyWork or prove cloud login, actual Noise, phone/WAN or long-run acceptance. Workflow definitions and local runs must not be described as completed Actions runs.

Only the final publishing job has `contents: write`. It waits for the three packaging jobs and the reusable integration gate, merges their artifacts, creates the Android convenience ZIP, and writes `release-manifest.json` and `SHA256SUMS`. The manifest records the actual Git commit and the independent component versions. A draft Release receives every asset before it becomes public. An existing Release is not overwritten automatically.

Actions stores only one copy of the APK between jobs; the convenience ZIP is created after download in the final job. Intermediate artifacts expire after one day. Release assets are separate from Actions artifacts.

## Create a project release

From the reviewed, clean repository:

```sh
# Push only the reviewed public snapshot commit based on the published baseline.
# Do not push the private local development history.
git push origin main
gh workflow run release.yml --ref main -f release_tag=v0.8.0
```

`v0.8.0` is the proposed target; recheck it is unused before dispatch: choose a new, unused project tag after review. Dispatch from the exact reviewed source revision. The publishing job creates the project tag only after all required builds have passed, and binds the Release to that job's `GITHUB_SHA`. Do not create or move a release tag during build iteration. Existing tags remain unchanged. The Go submodule uses its independent `sdk/v0.4.0` tag; create a new submodule tag only for a new Go SDK version.

Check that all required jobs succeeded and the Release is published at [GitHub Releases](https://github.com/ywateru/CyberGrid/releases). Download `SHA256SUMS` with the desired asset and verify it:

```sh
shasum -a 256 -c SHA256SUMS
```

That command expects all listed files in the current directory. To verify a single downloaded asset, select its exact line from the checksum file or compare its SHA256 with `release-manifest.json`.

## Build or package locally

Install the fixed toolchains from the workflow, use the committed dependency locks, then run the component build instructions. For an Android build, set `JAVA_HOME`, `ANDROID_HOME` and `NDK_HOME` to your own installations. Both SDKs ship an offline preparation script:

```sh
python3 sdk-rust/scripts/prepare-android-provider.py /absolute/path/to/host/app/src/main/jniLibs
```

Release packaging reads the committed source list through `git ls-files`. It does not reuse a previous local verification manifest or infer a source freeze from an old package:

```sh
# Build the website/service and JS SDK before packaging sources.
npm --prefix server ci --include=dev
npm --prefix server run build
npm --prefix sdk-js ci --include=dev --ignore-scripts
npm --prefix sdk-js test
npm --prefix sdk-js run test:integration
REQUIRE_REDIS=1 npm --prefix sdk-js run test:bridge-integration
python3 scripts/release/package.py sources
python3 scripts/release/package.py macos --app '/absolute/path/to/CyberGrid Lab.app' --go-binary sdk/bin/cybergrid-macos
python3 scripts/release/package.py android --apk /absolute/path/to/app-universal-debug.apk --go-binary sdk/bin/cybergrid-android-arm64
RELEASE_TAG=v0.8.0 python3 scripts/release/package.py finalize
```

Output is under `dist/release/`, which is not committed. `finalize` requires all three platform/source job metadata files. Source archives contain the offline SDK provider resources and licenses. Platform packages include full notices; the macOS CLI embeds its provider, while Android requires the OS-installed native library directory.

The JS outputs are `cybergrid-sdk-js-0.2.0-source.tar.gz`, its source manifest, and `cybergrid-sdk-js-0.2.0-esm.tar.gz`. The ESM archive includes exactly `index`, `bridge`, `directory` and `endpoints`, each as `.js` and `.d.ts`, plus package metadata, README, LICENSE, notices and build provenance. It contains no `node_modules` or user credentials. The source package remains private to prevent accidental npm publication; consumers may install the extracted local package. A separate `sdk-js.yml` workflow also checks source changes without publishing a release.

## Test-build limitations

The macOS app is an arm64 debug-test build and is not notarized. The Android APK is an arm64 debug-test build with an explicit ADB control feature and an ephemeral CI debug signing key. A key change prevents in-place Android upgrades; users must handle existing app credentials with the original app before choosing uninstall/reinstall. These builds do not publish signing keys or personal app data.

CI performs actual compilation and packaging. It does not automatically authorize Logto grants, run phones, repeat WAN tests, or claim new runtime acceptance from a successful build. The website and API run in Node.js / Docker or Vercel Functions, with Redis as the shared coordination store. The retained `memory-v1` wire identifier does not select an in-process RAM store. SDK/App binaries are downloaded from GitHub Releases rather than stored in the service image. A separate server workflow checks the Node entrypoint and Docker image without triggering native releases.

## Public snapshot and history gate

Before a final release freeze, review the explicit `scripts/release/source-policy.json`, generate a fresh plan, export the snapshot and scan it with the current known-private values kept only in private memory/files. The plan and exporter both reject binaries outside the reviewed roots or exact exceptions, including any additional sample icon. These checks do not inspect `.git` or prove history safe. `package.py` uses tracked working-tree bytes and records HEAD; it requires a clean, reviewed checkout to make those bytes equal that commit. It does not replace the snapshot/history gate.

Unpublished local history may contain reports later redacted in the current tree. Do not push that history on the strength of a current-tree scan. Preserve the private original freeze/evidence and, when necessary, create a reviewed public snapshot commit from the published baseline; record the local commit, exported tree and public commit mapping and verify actual public blobs. Do not rewrite existing published tags/history. Task012's intermediate snapshot excludes later task010 work: regenerate the allowlist, export, privacy/license checks and package hashes after the final sample freeze.

The standalone Rust source archive lacks repository Node/Redis prerequisites. Its default ignored capability tests are an explicitly incomplete repository check; the full repository gate above must run them. `scripts/release/RELEASE-NOTES.md` prepares v0.8.0 and its independently versioned components; it is not evidence that the release exists or task010 is complete. Source changes in static products, finite admission, gates or browser adapters must not be presented as already present in immutable v0.7.0 downloads.

## Complete repository source

The prepared `CyberGrid-0.8.0-source.tar.gz` and `cybergrid-source-manifest.json` include only tracked inputs accepted by the explicit public policy. The embedded `SOURCE-MANIFEST.json` records every source hash and the actual public commit; the external manifest has identical bytes. This repository layout preserves sample relative imports into the actual SDKs and server. It includes `examples/ram-only-workspace/web/.env.example` as an exact public exception; other environment files remain excluded. No separate sample ZIP, private adapter or external AGPL implementation is included. The archive is source, not a preconfigured production deployment or complete real-world sample acceptance.

After extracting the archive, install locked server, sdk-js, desktop and Web dependencies, build the server/JS SDK, then execute the sample commands below with actual Redis available. These commands are repository integration checks, not cloud/phone/long-run evidence:

```sh
export REQUIRE_REDIS=1
npm --prefix server ci --include=dev --ignore-scripts
npm --prefix server run build
npm --prefix sdk-js ci --include=dev --ignore-scripts
npm --prefix sdk-js run build
npm --prefix examples/ram-only-workspace/desktop ci --include=dev --ignore-scripts
npm --prefix examples/ram-only-workspace/web ci --include=dev --ignore-scripts
npm --prefix examples/ram-only-workspace/desktop test
npm --prefix examples/ram-only-workspace/desktop run build
npm --prefix examples/ram-only-workspace/web test
npm --prefix examples/ram-only-workspace/web run test:integration
npm --prefix examples/ram-only-workspace/web run build
cargo test --locked --manifest-path examples/ram-only-workspace/Cargo.toml --workspace -- --include-ignored
```

`finalize` verifies the exact version-derived asset set, including the complete source archive/manifest, and hashes each actual asset; it does not infer success from an old asset count. It refuses a release tag different from the prepared project version. The final publishing gate still requires integration-010 acceptance and independent review.
