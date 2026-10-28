# Changelog

All notable changes to this project are documented here, following
[Keep a Changelog](https://keepachangelog.com/) and [Semantic Versioning](https://semver.org/).
The app shows this file in Settings.

## Unreleased

## 0.7.1 - 2026-10-28

### Fixed
- Waiting Flatpak updates are checked per installation, so "Flatpak (user)" and "Flatpak (system)" no longer both list every update from both.

## 0.7.0 - 2026-10-25

### Added
- Waiting updates per source on Overview. Sources that can be checked (system packages with dnf, Flatpak, Homebrew formulae and apps, npm, pip3, snap, macOS system updates, rustup) show a count tag; click it to list what would be updated. Others show a check when up to date, a warning when the check failed, or a dashed tag when they can't be checked. Checks run in the background about once a day, after each update and with the refresh button; they never ask for a password.
- The "time to update" reminder mentions how many updates are waiting.

### Changed
- Checkboxes match the app's theme.
- The terminal at the bottom always starts collapsed, and collapses again when the window is closed to the tray (unless a run is active). It no longer remembers being open.

## 0.6.3 - 2026-10-25

### Removed
- The "Check what would change" button. A topgrade dry run only lists the tools it would run, not which packages would change, and Overview's source list already shows that. Past previews still show in History.

## 0.6.2 - 2026-10-25

### Fixed
- Homebrew's upgrade question is now really recognised: Homebrew ends the question with a line break before waiting for the answer, which 0.6.1 still missed, so the run seemed to hang without a notification.

## 0.6.1 - 2026-10-24

### Fixed
- Homebrew's "Do you want to proceed with the upgrade? [y/n]" (and apt's "[Y/n]" without a colon) is now recognised as a question: the app shows "A question from topgrade" and notifies you instead of seeming to hang.

## 0.6.0 - 2026-10-24

### Added
- "What changed" page after each run: every updated package with its old and new version, grouped into Apps, System and tools, and Firmware, with a search box, filters and "Copy list". A one-sentence summary names the apps you'll recognise.
- A "Restart when convenient" note when firmware or core system packages were updated.
- Password prompts appear as a field in the app; the password is passed to sudo in the terminal and not stored.
- Overview lists what topgrade updates on this computer, found with a background dry run (changes nothing, needs no password) and refreshed daily or with its refresh button.
- Theme setting: System, Light or Dark. On Linux the tray icon keeps following the desktop panel's theme.

### Changed
- Tray menu reordered: "Last update" first, then "Update everything" (was "Run now"; shows "Show progress" during a run) and "Open Topgrade GUI"; "Check for app updates…" (was "Check for updates") and Quit sit apart below.
- A run that was stopped before topgrade's summary counts as not finished: Overview says the last update didn't finish, History marks it "Didn't finish", and the update reminder (and "Last update" in the tray menu) count from the last finished run instead.
- New layout with Overview, History and Settings pages. Overview shows whether an update is due, the run options in plain words, and the last run's results; during a run it shows progress and the steps so far.
- The terminal moved into a drawer at the bottom that you can open and close; clicking a step opens it at that step's output.
- The app's own logo and colours are used throughout.
- Escape (and the other back gestures) also closes the results after a run or preview and returns to Overview. On the History list or Settings, Escape returns to Overview too. On Overview itself, Escape closes the window (the app keeps running in the tray).
- Escape during a run or preview asks "Stop this update?" before stopping it (Keep running is the default). While you're typing in the terminal, Escape goes to the terminal instead.
- Past runs open as a sub-page with a breadcrumb and a Back button that returns to where you came from (History or Overview); Escape, the mouse back button and Alt+← (⌘[ on macOS) work too.
- The app calls itself "Topgrade GUI" throughout, to keep it apart from topgrade, the tool it runs.
- No separate title bar: the app's header is the title bar (drag to move, double-click to maximize). Linux gets the app's own minimize, maximize and close buttons; macOS keeps its native window buttons, centred in the header, and shows only the logo there (the menu bar already names the app).

## 0.5.0 - 2026-10-24

### Added
- When topgrade waits for input (sudo password, yes/no, retry) the tray icon pulses, and a notification (plus Dock bounce on macOS) appears if the window is not in front.
- A banner and a coloured footer line in the terminal show when a run has ended and how it went.

### Changed
- Smoother tray icon animation (more frames, gradual glow).

### Fixed
- Topgrade runs in an interactive login shell so PATH set in `.zshrc` (e.g. pnpm's bin dir) is available; fixes `ERR_PNPM_GLOBAL_BIN_DIR_NOT_IN_PATH`.

## 0.4.6 - 2026-10-24

### Fixed
- macOS builds are ad-hoc code signed so the bundle is sealed; README explains how to open the unnotarized app.

## 0.4.5 - 2026-10-24

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
