# Backlog

Ideas and planned work, roughly in priority order. Move an item to
`CHANGELOG.md` (under Unreleased) when it ships.

## More waiting-update checks

Overview shows waiting updates for dnf, Flatpak, Homebrew, npm and rustup
(`src-tauri/src/checks.rs`). Sources without a check show a dashed tag. Add
checks for more of them, each with a parser and fixture:

- `softwareupdate -l` (macOS system update)
- `pip list --outdated` (pip3), `snap refresh --list`, `fwupdmgr get-updates`
- pnpm, Yarn and Bun global packages
- The Homebrew check is only accurate after `brew update`; consider running
  it first.
- The Flatpak check needs network; offline shows "couldn't check".
