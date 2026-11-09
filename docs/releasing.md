# Building and publishing a release

CyberGrid uses a project release tag, such as `v0.6.0`, while the components retain their own versions: Go SDK/CLI 0.4.0, Rust SDK 0.2.0 and CyberGrid Lab 0.3.0. Publishing source does not grant an additional open-source license. See the root and component `LICENSE` files; third-party grants remain separately effective.

The v0.6.0 coordinator and Lab use ordinary Logto Native applications with Device Flow disabled. Deploy the matching coordinator with the client release. Historical device-code CLI login does not work with this coordinator; it remains only for older compatible services. Existing releases are immutable.

## Workflow

The `Build and publish native SDKs` workflow in `.github/workflows/release.yml` runs for project tags matching `v*`, or through `workflow_dispatch` with an explicit project release tag. The Go submodule tag `sdk/v0.4.0` does not match that trigger, so it does not start a duplicate project release.

The workflow has three required build jobs:

- **sources**, on Linux: build the website and typecheck the Node service, verify both fixed provider resources, then package committed Go/Rust/App source and original license texts.
- **macos**, on a native macOS arm64 runner: fixed Node 22.23.4, Go 1.27.0 and Rust 1.97.1; build the Go CLI and Tauri 2 debug-test app.
- **android**, on Linux: the same language toolchains, JDK 17, Android SDK Platform 37.0, Build-Tools 36.0.0 and NDK 27.2.12479018; build the Go arm64 CLI with the API28 NDK compiler and bionic DNS, plus the arm64 API28+ Tauri debug-test APK with `adb-test-control` enabled.

Actions are referenced by full commit hashes. npm uses `npm ci`; Cargo and Go dependencies have committed lock/checksum files. The fixed platform provider bytes are already in the repository. `scripts/prepare-android-provider.sh` copies the verified resource into the app's generated `jniLibs` directory; the APK contains one `libcloudflared.so`. No cloudflared binary is downloaded at runtime.

Only the final publishing job has `contents: write`. It waits for all three jobs, merges their artifacts, creates the Android convenience ZIP, and writes `release-manifest.json` and `SHA256SUMS`. The manifest records the actual Git commit and the independent component versions. A draft Release receives every asset before it becomes public. An existing Release is not overwritten automatically.

Actions stores only one copy of the APK between jobs; the convenience ZIP is created after download in the final job. Intermediate artifacts expire after one day. Release assets are separate from Actions artifacts.

## Create a project release

From the reviewed, clean repository:

```sh
git push origin main
gh workflow run release.yml --ref main -f release_tag=v0.6.0
```

Dispatch from the exact reviewed source revision. The publishing job creates the project tag only after all required builds have passed, and binds the Release to that job's `GITHUB_SHA`. Do not create or move a release tag during build iteration. Existing tags remain unchanged. The Go submodule uses its independent `sdk/v0.4.0` tag; create a new submodule tag only for a new Go SDK version.

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
python3 scripts/release/package.py sources
python3 scripts/release/package.py macos --app '/absolute/path/to/CyberGrid Lab.app' --go-binary sdk/bin/cybergrid-macos
python3 scripts/release/package.py android --apk /absolute/path/to/app-universal-debug.apk --go-binary sdk/bin/cybergrid-android-arm64
RELEASE_TAG=v0.6.0 python3 scripts/release/package.py finalize
```

Output is under `dist/release/`, which is not committed. `finalize` requires all three platform/source job metadata files. Source archives contain the offline SDK provider resources and licenses. Platform packages include full notices; the macOS CLI embeds its provider, while Android requires the OS-installed native library directory.

## Test-build limitations

The macOS app is an arm64 debug-test build and is not notarized. The Android APK is an arm64 debug-test build with an explicit ADB control feature and an ephemeral CI debug signing key. A key change prevents in-place Android upgrades; users must handle existing app credentials with the original app before choosing uninstall/reinstall. These builds do not publish signing keys or personal app data.

CI performs actual compilation and packaging. It does not automatically authorize Logto grants, run phones, repeat WAN tests, or claim new runtime acceptance from a successful build. The website and API run in one persistent Node process (directly or in Docker). SDK/App binaries are downloaded from GitHub Releases rather than stored in the service image. A separate server workflow checks the Node entrypoint and Docker image without triggering native releases.
