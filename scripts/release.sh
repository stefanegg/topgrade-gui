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

# First `"version"` / `version =` line only (never a dependency's). Perl
# rather than sed so this runs the same on macOS and Linux.
export version date="$(date +%F)"
for f in package.json src-tauri/tauri.conf.json; do
  perl -0pi -e 's/^  "version": ".*?"/  "version": "$ENV{version}"/m' "$f"
done
perl -0pi -e 's/^version = ".*?"/version = "$ENV{version}"/m' src-tauri/Cargo.toml

perl -0pi -e 's/^## Unreleased$/## Unreleased\n\n## $ENV{version} - $ENV{date}/m' CHANGELOG.md

# Refresh the lockfile's own package entry.
(cd src-tauri && cargo update --offline -p topgrade-gui >/dev/null)

git add package.json CHANGELOG.md src-tauri/Cargo.toml src-tauri/Cargo.lock src-tauri/tauri.conf.json
git commit -q -m "Release v$version"
git tag -a "v$version" -m "v$version"
echo "Tagged v$version. Build with: pnpm tauri build   Publish with: git push --follow-tags"
