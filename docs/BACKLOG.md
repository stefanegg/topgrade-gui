# Backlog

Ideas and planned work, roughly in priority order. Move an item to
`CHANGELOG.md` (under Unreleased) when it ships.

## More waiting-update checks

Overview shows waiting updates for dnf, Flatpak, Homebrew, npm, pip3, snap,
macOS system updates and rustup
(`src-tauri/src/checks.rs`). Sources without a check show a dashed tag. Add
checks for more of them, each with a parser and fixture:

- `fwupdmgr get-updates` (firmware); needs a fixture with an update
- pnpm, Yarn and Bun global packages (`pnpm outdated -g` fails without a
  global manifest)
- Verify the macOS `softwareupdate --list` and Homebrew parsers against real
  output; their fixtures are hand-written.
- The Homebrew check is only accurate after `brew update`; consider running
  it first.
- The Flatpak check needs network; offline shows "couldn't check".
