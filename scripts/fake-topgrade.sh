#!/usr/bin/env bash
# Dev-only stand-in for topgrade that asks for input, to test the waiting
# state (tray pulse, notification, Dock bounce). Debug builds use it when
# TOPGRADE_GUI_BIN points here:
#   TOPGRADE_GUI_BIN="$PWD/scripts/fake-topgrade.sh" pnpm tauri dev
# DELAY=<seconds> sets the pause before each prompt (default 6).

if [ "$1" = "--version" ]; then
  echo "topgrade 0.0.0-fake"
  exit 0
fi

delay=${DELAY:-6}

header() {
  printf '\n── %s - %s ──────────────────────────────────────────────────\n\n' "$(date +%H:%M:%S)" "$1"
}

header "System update"
echo "Checking for updates..."
sleep 1
echo "Needs administrator rights."
sleep "$delay"
printf '[sudo] password for %s: ' "${USER:-user}"
read -rs _
echo
echo "Thanks (the password is discarded)."

header "Fake packages"
echo "Upgrading foo 1.0 -> 1.1"
sleep 1
printf 'Proceed with removal of old versions? [y/N]: '
sleep "$delay"
read -r _
echo "Done."

header "Flaky step"
echo "error: something went wrong"
sleep 1
sleep "$delay"
printf 'Retry? (y)es/(N)o/(s)hell/(q)uit '
read -r _
echo

header "Summary"
echo "System update: OK"
echo "Fake packages: OK"
echo "Flaky step: FAILED"
