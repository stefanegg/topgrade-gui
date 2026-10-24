# Backlog

Ideas and planned work, roughly in priority order. Move an item to
`CHANGELOG.md` (under Unreleased) when it ships.

## Waiting-update counts for common package managers

Show how many updates are waiting per source ("System packages · 37
waiting"), so Overview and the tray reminder can say "37 updates waiting"
instead of only "it's been 7 days".

- One "Check for updates" action that runs all supported checks in
  parallel, plus an automatic check about once a day; no button per tool.
- Start with dnf (`dnf check-update`), Flatpak (`flatpak remote-ls
  --updates`) and Homebrew (`brew outdated --json`; accurate only after
  `brew update`). Other tools show no count.
- Run in the background: these refresh metadata and need network
  (dnf can take 10–30 s). Must never ask for a password.
- Each tool needs its own output parser and tests, like the extractors in
  `src-tauri/src/extractors/`.
