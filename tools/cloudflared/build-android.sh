#!/usr/bin/env bash
set -euo pipefail

# Rebuild official Cloudflare source, with Android bionic getaddrinfo enabled.
# No system DNS, route, certificate or proxy settings are changed.
CLOUDFLARED_VERSION=2026.9.0
CLOUDFLARED_COMMIT=b749dfd998f4bced81f48727b9c971cfc3056865
CLOUDFLARED_BUILD_TIME=2026-09-09T15:55:05Z
CLOUDFLARED_REPOSITORY=https://github.com/cloudflare/cloudflared.git
TOOL_DIR="$(cd "$(dirname "$0")" && pwd)"
GO_BIN="${GO_BIN:-go}"
ANDROID_API="${ANDROID_API:-28}"
NDK_ROOT="${ANDROID_NDK_HOME:-${ANDROID_HOME:-$HOME/Library/Android/sdk}/ndk/27.2.12479018}"
case "$(uname -s)-$(uname -m)" in
  Darwin-*) NDK_HOST=darwin-x86_64 ;;
  Linux-x86_64) NDK_HOST=linux-x86_64 ;;
  *) echo 'Unsupported NDK host; use a macOS or Linux x86_64 NDK toolchain.' >&2; exit 1 ;;
esac
CC_BIN="$NDK_ROOT/toolchains/llvm/prebuilt/$NDK_HOST/bin/aarch64-linux-android${ANDROID_API}-clang"
if ! command -v "$GO_BIN" >/dev/null; then echo 'Set GO_BIN to a Go 1.26+ executable.' >&2; exit 1; fi
if [ ! -x "$CC_BIN" ]; then echo 'Set ANDROID_NDK_HOME to NDK 27.2.12479018, or install that NDK.' >&2; exit 1; fi
TMP_BUILD="$(mktemp -d "${TMPDIR:-/tmp}/cybergrid-cloudflared.XXXXXXXX")"
trap 'rm -rf "$TMP_BUILD"' EXIT

git clone --quiet --depth 1 --branch "$CLOUDFLARED_VERSION" "$CLOUDFLARED_REPOSITORY" "$TMP_BUILD/source"
ACTUAL_COMMIT="$(git -C "$TMP_BUILD/source" rev-parse HEAD)"
if [ "$ACTUAL_COMMIT" != "$CLOUDFLARED_COMMIT" ]; then echo 'Official release tag commit differs from the pinned commit; refusing to build.' >&2; exit 1; fi
git -C "$TMP_BUILD/source" apply "$TOOL_DIR/patches/0001-android-explicit-dns.patch"
mkdir -p "$TOOL_DIR/bin"
(
  cd "$TMP_BUILD/source"
  # GOTOOLCHAIN=local prevents a hidden compiler upgrade. Dependencies are pinned by upstream go.sum.
  GOOS=android GOARCH=arm64 CGO_ENABLED=1 GOTOOLCHAIN=local CC="$CC_BIN" \
    "$GO_BIN" build -trimpath -buildvcs=false -mod=readonly \
    -ldflags "-s -w -X main.Version=${CLOUDFLARED_VERSION}-cybergrid-android-dns1 -X main.BuildTime=${CLOUDFLARED_BUILD_TIME}" \
    -o "$TOOL_DIR/bin/cloudflared-android-arm64" ./cmd/cloudflared
  cp LICENSE "$TOOL_DIR/LICENSE"
  # List exactly the modules linked into this binary, and retain their available copyright/license notices.
  "$GO_BIN" version -m "$TOOL_DIR/bin/cloudflared-android-arm64" > "$TOOL_DIR/build-modules.txt"
  GOTOOLCHAIN=local "$GO_BIN" list -m -json all > "$TMP_BUILD/modules.json"
)
python3 "$TOOL_DIR/write-manifest.py" "$TMP_BUILD/source" "$TMP_BUILD/modules.json" "$CC_BIN" "$GO_BIN"
printf 'Built: %s\n' "$TOOL_DIR/bin/cloudflared-android-arm64"
