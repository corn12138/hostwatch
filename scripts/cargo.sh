#!/bin/sh
set -eu
project_root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
cd "$project_root"

# Cargo adds target paths to DYLD_FALLBACK_LIBRARY_PATH, where ':' is a separator.
# Keep this checkout's artifacts in a distinct cache without renaming the user's directory.
case "$project_root" in
  *:*)
    if [ -z "${CARGO_TARGET_DIR:-}" ]; then
      project_key=$(printf '%s' "$project_root" | cksum | awk '{print $1}')
      export CARGO_TARGET_DIR="${HOME}/Library/Caches/hostwatch/build-${project_key}"
    fi
    ;;
esac
exec cargo "$@"
