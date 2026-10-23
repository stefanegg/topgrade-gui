#!/usr/bin/env bash
# Cut a release: scripts/release.sh X.Y.Z
#
# Writes X.Y.Z into package.json, src-tauri/Cargo.toml (+ Cargo.lock) and
# src-tauri/tauri.conf.json, dates the changelog's Unreleased entries, commits
# "Release vX.Y.Z" and tags it. Building that tag then yields bundles and an
# in-app version that all say X.Y.Z. Push with: git push --follow-tags
set -euo pipefail

version="${1:-}"
if [[ ! "$version" =~ ^[0-9]+\.[0-9]+\.[0-9]+$ ]]; then
  echo "usage: $0 X.Y.Z" >&2
  exit 1
fi

cd "$(dirname "$0")/.."

if [[ -n "$(git status --porcelain)" ]]; then
  echo "working tree is not clean" >&2
  exit 1
fi
if git rev-parse -q --verify "refs/tags/v$version" >/dev/null; then
  echo "tag v$version already exists" >&2
  exit 1
fi

# First `"version"` / `version =` line only (never a dependency's).
sed -i.bak -E '0,/^  "version": ".*"/s//  "version": "'"$version"'"/' package.json src-tauri/tauri.conf.json
sed -i.bak -E '0,/^version = ".*"/s//version = "'"$version"'"/' src-tauri/Cargo.toml
rm -f package.json.bak src-tauri/tauri.conf.json.bak src-tauri/Cargo.toml.bak

sed -i.bak "s/^## Unreleased\$/## Unreleased\n\n## $version - $(date +%F)/" CHANGELOG.md
rm -f CHANGELOG.md.bak

# Refresh the lockfile's own package entry.
(cd src-tauri && cargo update --offline -p topgrade-gui >/dev/null)

git add package.json CHANGELOG.md src-tauri/Cargo.toml src-tauri/Cargo.lock src-tauri/tauri.conf.json
git commit -q -m "Release v$version"
git tag -a "v$version" -m "v$version"
echo "Tagged v$version. Build with: pnpm tauri build   Publish with: git push --follow-tags"
