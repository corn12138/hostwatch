#!/bin/sh
# Builds Hostwatch.app from this checkout, replaces the installed copy and starts it.
# Usage: scripts/install.sh [destination folder, default /Applications]
set -eu
project_root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
cd "$project_root"
destination=${1:-/Applications}
pattern='Hostwatch.app/Contents/MacOS/hostwatch'

./scripts/package-app.sh

if pgrep -f "$pattern" >/dev/null; then
  pkill -TERM -f "$pattern" || true
  for _ in 1 2 3 4 5 6 7 8 9 10; do
    pgrep -f "$pattern" >/dev/null || break
    sleep 0.5
  done
  if pgrep -f "$pattern" >/dev/null; then
    echo "Hostwatch is still running; quit it and try again." >&2
    exit 1
  fi
fi

rm -rf "$destination/Hostwatch.app"
ditto dist/Hostwatch.app "$destination/Hostwatch.app"
open "$destination/Hostwatch.app"
echo "Installed and started $destination/Hostwatch.app"
