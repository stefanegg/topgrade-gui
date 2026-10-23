# Topgrade GUI

Desktop GUI for [topgrade](https://github.com/topgrade-rs/topgrade) (Tauri + Svelte), for macOS and Fedora KDE.

- Runs topgrade in an embedded terminal (type sudo passwords and answers there)
- Splits the output into steps with status, duration and what each step updated
- Keeps the last 12 runs with their logs and summaries (History)
- Lives in the tray: Run now, last run, and a reminder when an update is overdue
- Zoom the terminal font with Shift + mouse wheel (remembered)
- Updates itself from signed GitHub Releases (asks first)

Requires `topgrade` on your login shell's `PATH`.

> **Note:** this is a personal project, published as is. There is no support, and issues and pull requests are not monitored. Feel free to fork it.

## Install

Download the latest installer from the [Releases](https://github.com/stefanegg/topgrade-gui/releases) page:

- **Fedora:** `sudo dnf install ./Topgrade.GUI-*.rpm` (a `.deb` and an `.AppImage` are available too)
- **macOS (Apple Silicon):** open the `.dmg` and copy the app to `/Applications` before the first launch (the login item points to the app's location). The build is unsigned, so macOS may ask you to confirm the first start in System Settings → Privacy & Security.

The app then keeps itself up to date, see [Auto-update](#auto-update).

## Build from source

### macOS

```bash
pnpm install
pnpm build:mac
```

Outputs `src-tauri/target/release/bundle/macos/Topgrade GUI.app` and a `.dmg` in `bundle/dmg/`. Copy the app to `/Applications` before the first launch (the login item points to the app's location). A locally built app runs without Gatekeeper prompts.

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

To release, add entries under `Unreleased` in `CHANGELOG.md`, then run `scripts/release.sh X.Y.Z` from a clean tree. It writes the version into `package.json`, `Cargo.toml`, `Cargo.lock` and `tauri.conf.json`, dates the changelog entry, commits and tags `vX.Y.Z`. Building that tag (`pnpm tauri build`) gives bundle file names and an in-app version that all read `X.Y.Z`. Publish with `git push --follow-tags`: pushing the tag starts `.github/workflows/release.yml`, which builds the Linux (AppImage, deb, rpm) and macOS (dmg) installers and attaches them to a GitHub Release with the changelog section as notes. To (re)build an existing tag, run the workflow manually with the tag name.

## Auto-update

The app checks `releases/latest/download/latest.json` on GitHub (first after 2 minutes, then every 6 hours; Settings can turn this off), shows a banner in the window and an "Install update …" tray item, and on confirmation downloads the update, verifies its signature against the public key in `tauri.conf.json`, installs it and restarts. AppImage and macOS builds are replaced in place; an RPM install is upgraded through `pkexec` (password prompt). Updates are refused while a topgrade run is active.

Release builds are signed in CI with the `TAURI_SIGNING_PRIVATE_KEY` repository secret. The private key lives in `~/.tauri/topgrade-gui.key` on the maintainer's machine — keep a backup; without it installed apps can't be updated. Local `pnpm build:linux` / `pnpm build:mac` runs need it too (they create update artifacts): `export TAURI_SIGNING_PRIVATE_KEY=~/.tauri/topgrade-gui.key TAURI_SIGNING_PRIVATE_KEY_PASSWORD=`.

## Data

`~/Library/Application Support/dev.topgradegui.desktop/` (macOS) or `~/.local/share/dev.topgradegui.desktop/` (Linux): `settings.json` and `logs/` (raw log + JSON summary per run).

The window reopens with its last size and position (`window.json`), unless the monitor setup changed since. On Linux the app runs via XWayland so it can place its window (set `GDK_BACKEND` to override).
