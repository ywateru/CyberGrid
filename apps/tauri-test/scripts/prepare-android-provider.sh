#!/bin/sh
# Stage exactly one pinned provider from the SDK's offline distribution.
set -eu
TASK_APP_ROOT=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
TASK_REPO_ROOT=$(CDPATH= cd -- "$TASK_APP_ROOT/../.." && pwd)
exec python3 "$TASK_REPO_ROOT/sdk-rust/scripts/prepare-android-provider.py" \
  "$TASK_APP_ROOT/src-tauri/gen/android/app/src/main/jniLibs"
