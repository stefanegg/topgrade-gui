# Changelog

All notable changes to this project are documented here, following
[Keep a Changelog](https://keepachangelog.com/) and [Semantic Versioning](https://semver.org/).
The app shows this file in Settings.

## Unreleased

### Added
- The tray icon animates while a run is active: a glow climbs through the arrows.

## 0.4.4 - 2026-10-23

### Fixed
- macOS build failed after the single-instance change in 0.4.3; no macOS build of 0.4.3 was published.

## 0.4.3 - 2026-10-23

### Fixed
- Only one instance of the app runs at a time; launching it again brings up the existing window.

## 0.4.2 - 2026-10-23

### Changed
- Toolbar options now use plain-language labels in the normal UI font (with the topgrade flag in the tooltip) instead of bare monospaced flags.
- On Linux the tray icon is a flat white or black glyph matching the light/dark theme instead of the colorful app icon.

### Fixed
- The window now sets its icon explicitly, so the taskbar no longer shows a blank icon on Linux.

## 0.4.1 - 2026-10-23

### Changed
- The window now runs with a strict Content Security Policy.

### Fixed
- Installing an update no longer stops a topgrade run that was started while the update was downloading; the update then takes effect on the next launch.

## 0.4.0 - 2026-10-23

### Changed
- The app identifier is now `dev.topgradegui.desktop`, so the data folder moved; copy the contents of the old folder over to keep settings and history.

## 0.3.0 - 2026-10-23

### Added
- Auto-update: new releases are detected in the background (and via "Check for updates" in the tray menu and Settings), announced with a notification, and installed from the signed GitHub Release after confirmation. Can be switched off in Settings.
- GitHub Actions workflow that builds installers and publishes a GitHub Release for every version tag.

## 0.2.0 - 2026-10-23

### Added
- Zoom the terminal font with Shift + mouse wheel; the size is remembered.
- Version (from git tags) and changelog in Settings.

## 0.1.0 - 2026-10-09

### Added
- Run topgrade in a PTY with live xterm.js output and interactive prompts.
- Parse output into steps, summary results and per-step updates.
- Save raw logs and run summaries; history view.
- Tray icon, overdue-update reminder, start at login and settings.
- macOS app packaging (login item via SMAppService).
- Remember window size and position across launches.
