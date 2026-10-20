#!/bin/sh
# Builds dist/Hostwatch.app and a zip for this machine's architecture. The app is ad-hoc signed
# only (no Developer ID, not notarized), so Gatekeeper asks for confirmation on first launch.
set -eu
project_root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
cd "$project_root"
version=$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -1)
arch=$(uname -m)
./scripts/cargo.sh build --release --locked
target_dir=${CARGO_TARGET_DIR:-}
if [ -z "$target_dir" ]; then
  target_dir=$(./scripts/cargo.sh metadata --format-version 1 --no-deps | sed -n 's/.*"target_directory":"\([^"]*\)".*/\1/p')
fi
app=dist/Hostwatch.app
rm -rf "$app"
mkdir -p "$app/Contents/MacOS"
cp "$target_dir/release/hostwatch" "$app/Contents/MacOS/hostwatch"
sed "s/VERSION/$version/g" packaging/Info.plist > "$app/Contents/Info.plist"
codesign --force --sign - "$app"
zip_name="Hostwatch-$version-macos-$arch.zip"
rm -f "dist/$zip_name"
(cd dist && ditto -c -k --keepParent Hostwatch.app "$zip_name")
shasum -a 256 "dist/$zip_name"
