# Topgrade GUI

Desktop GUI for [topgrade](https://github.com/topgrade-rs/topgrade) (Tauri + Svelte), for macOS and Fedora KDE.

- Runs topgrade in an embedded terminal (type sudo passwords and answers there)
- Splits the output into steps with status, duration and what each step updated
- Keeps the last 12 runs with their logs and summaries (History)
- Lives in the tray: Run now, last run, and a reminder when an update is overdue

Requires `topgrade` on your login shell's `PATH`.

## Build and install

### macOS

```bash
pnpm install
pnpm build:mac
```

Outputs `src-tauri/target/release/bundle/macos/Topgrade GUI.app` and a `.dmg` in `bundle/dmg/`. Copy the app to `/Applications` before the first launch (the login item points to the app's location). The build is unsigned; a locally built app runs without Gatekeeper prompts.

### Fedora

Build dependencies:

```bash
sudo dnf install webkit2gtk4.1-devel openssl-devel curl wget file libappindicator-gtk3-devel librsvg2-devel libxdo-devel
sudo dnf group install c-development
```

Plus Rust (rustup), Node.js and pnpm. Then:

```bash
pnpm install
pnpm build:linux
sudo dnf install ./src-tauri/target/release/bundle/rpm/*.rpm
```

An AppImage is also built in `bundle/appimage/`.

## Develop

```bash
pnpm tauri dev
```

Dev builds have a "Replay fixture…" menu that plays the logs in `src-tauri/tests/fixtures/` through the UI. Tests: `cd src-tauri && cargo test`.

## Versioning and releases

The app version is derived from git tags at build time (`src-tauri/build.rs`) and shown in Settings: a tagged commit `v1.2.3` is `1.2.3`; later commits are `1.2.3-dev.N+gSHA`; uncommitted changes add `+dirty`. Without any `v*` tag it falls back to the version in `Cargo.toml`.

To release, add entries under `Unreleased` in `CHANGELOG.md`, then run `scripts/release.sh X.Y.Z` from a clean tree. It writes the version into `package.json`, `Cargo.toml`, `Cargo.lock` and `tauri.conf.json`, dates the changelog entry, commits and tags `vX.Y.Z`. Building that tag (`pnpm tauri build`) gives bundle file names and an in-app version that all read `X.Y.Z`. Publish with `git push --follow-tags`.

## Data

`~/Library/Application Support/dev.eggenschwiler.topgrade-gui/` (macOS) or `~/.local/share/dev.eggenschwiler.topgrade-gui/` (Linux): `settings.json` and `logs/` (raw log + JSON summary per run).

The window reopens with its last size and position (`window.json`), unless the monitor setup changed since. On Linux the app runs via XWayland so it can place its window (set `GDK_BACKEND` to override).
