#!/usr/bin/env bash
# Run native credential-store tests with an isolated Linux Secret Service.
set -euo pipefail

if [ "$#" -eq 0 ]; then
  echo "Usage: with-test-keyring.sh <command> [args...]" >&2
  exit 2
fi

if [ "$(uname -s)" != Linux ]; then
  exec "$@"
fi

for tool in dbus-run-session gnome-keyring-daemon dbus-send; do
  if ! command -v "$tool" >/dev/null 2>&1; then
    printf 'Credential-store tests require %s. Install dbus and gnome-keyring (Ubuntu/Debian: sudo apt-get install dbus gnome-keyring), then rerun this command.\n' "$tool" >&2
    exit 1
  fi
done

baml_test_state="$(mktemp -d "${TMPDIR:-/tmp}/baml-test-keyring.XXXXXX")"
trap 'rm -rf "$baml_test_state"' EXIT
export XDG_DATA_HOME="$baml_test_state/data"
export XDG_RUNTIME_DIR="$baml_test_state/runtime"
mkdir -m 700 "$XDG_DATA_HOME" "$XDG_RUNTIME_DIR"

dbus-run-session -- bash -euo pipefail -c '
  printf "%s\n" "baml-test-keyring" | gnome-keyring-daemon --unlock --components=secrets --daemonize
  dbus-send --session --print-reply --dest=org.freedesktop.secrets \
    /org/freedesktop/secrets org.freedesktop.DBus.Peer.Ping >/dev/null
  "$@"
' baml-test-keyring "$@"
