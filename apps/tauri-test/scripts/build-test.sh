#!/bin/sh
# Build test artifacts. This script never installs or starts either application.
set -eu
TASK_APP_ROOT=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
TASK_REPO_ROOT=$(CDPATH= cd -- "$TASK_APP_ROOT/../.." && pwd)
export CARGO_TARGET_DIR=${CARGO_TARGET_DIR:-"$TASK_REPO_ROOT/.build/tauri-target"}
cd "$TASK_APP_ROOT"
case "${1:-}" in
  macos)
    npm run tauri -- build --debug --bundles app
    ;;
  android)
    : "${JAVA_HOME:?Set JAVA_HOME to JDK 17}"
    : "${ANDROID_HOME:?Set ANDROID_HOME to the Android SDK}"
    : "${NDK_HOME:?Set NDK_HOME to NDK 27.2.12479018}"
    sh scripts/prepare-android-provider.sh
    npm run tauri -- android build --debug --target aarch64 --apk --features adb-test-control
    ;;
  *)
    echo 'Usage: sh scripts/build-test.sh macos|android' >&2
    exit 2
    ;;
esac
