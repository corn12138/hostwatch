#!/bin/sh
# Bumps the version, runs the checks, commits, tags vX.Y.Z and pushes. Pushing the tag starts
# the Release workflow, which builds Hostwatch.app and attaches it to a GitHub release.
# Usage: scripts/release.sh 0.1.1
set -eu
project_root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
cd "$project_root"
version=${1:?usage: scripts/release.sh X.Y.Z}
case "$version" in
  *[!0-9.]* | .* | *. | *..*) echo "version must look like 1.2.3" >&2; exit 1 ;;
esac
if [ -n "$(git status --porcelain --untracked-files=no)" ]; then
  echo "Commit or stash tracked changes first." >&2
  exit 1
fi
if git rev-parse -q --verify "refs/tags/v$version" >/dev/null; then
  echo "Tag v$version already exists." >&2
  exit 1
fi

sed -i '' "1,/^version = /s/^version = \".*\"/version = \"$version\"/" Cargo.toml
./scripts/cargo.sh fmt -- --check
./scripts/cargo.sh clippy --all-targets -- -D warnings
./scripts/cargo.sh test
git add Cargo.toml Cargo.lock
git commit -m "Release v$version"
git tag -a "v$version" -m "Hostwatch $version"
git push origin HEAD "v$version"
echo "Pushed v$version; the Release workflow will publish Hostwatch.app."
