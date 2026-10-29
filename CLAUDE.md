# topgrade-gui

Tauri 2 app (Rust backend in `src-tauri/`, SvelteKit + xterm.js frontend in `src/`). See README.md for build and run.

## Changelog

- Add a line under `## Unreleased` in `CHANGELOG.md` for every user-facing change (Added / Changed / Fixed), in the same commit as the change. The file is shown in the Settings screen, so keep the Keep a Changelog format (`## release`, `### group`, `- item`).

## Versioning and releases

- The app version comes from git tags (`vX.Y.Z`) via `src-tauri/build.rs`; don't hard-code it elsewhere.
- Never edit the version in `package.json`, `Cargo.toml`, `Cargo.lock` or `tauri.conf.json` by hand. Release only with `scripts/release.sh X.Y.Z`, which syncs them, dates the changelog, commits and tags. When asked to make or release a new version, that includes pushing the commit and tag (`git push --follow-tags`).
