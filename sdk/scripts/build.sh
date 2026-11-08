#!/bin/sh
set -eu
cd "$(dirname "$0")/.."
GO=${GO:-go}
mkdir -p bin
"$GO" build -trimpath -o bin/cybergrid-macos ./cmd/cybergrid
# Android requires bionic getaddrinfo: the pure-Go resolver cannot use Android
# netd and falls back to localhost:53, breaking public tunnel hostnames.
cybergrid_ndk=${CYBERGRID_ANDROID_NDK:-${ANDROID_NDK_HOME:-}}
if [ -z "$cybergrid_ndk" ]; then
  for candidate in "$HOME"/Library/Android/sdk/ndk/*; do
    if [ -d "$candidate/toolchains/llvm/prebuilt" ]; then cybergrid_ndk=$candidate; fi
  done
fi
if [ -z "$cybergrid_ndk" ]; then
  echo 'Set CYBERGRID_ANDROID_NDK to an Android NDK directory (API 28+).' >&2
  exit 1
fi
cybergrid_compiler=
for candidate in "$cybergrid_ndk"/toolchains/llvm/prebuilt/*/bin/aarch64-linux-android28-clang; do
  if [ -x "$candidate" ]; then cybergrid_compiler=$candidate; break; fi
done
if [ -z "$cybergrid_compiler" ]; then
  echo 'Android arm64 API28 NDK compiler not found.' >&2
  exit 1
fi
CGO_ENABLED=1 GOOS=android GOARCH=arm64 CC="$cybergrid_compiler" "$GO" build \
  -trimpath -ldflags=-checklinkname=0 -o bin/cybergrid-android-arm64 ./cmd/cybergrid
