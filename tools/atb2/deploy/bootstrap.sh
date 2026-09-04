#!/usr/bin/env bash
# Root only prepares ownership and starts children. No fetched code runs as root.
set -euo pipefail
umask 077
export PATH=/usr/local/cargo/bin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin
runner_home="${ATB2_HOME:-/data}"
if [ "$(id -u)" != 0 ] || [ "$runner_home" != /data ]; then
  echo 'atb2: bootstrap requires root and ATB2_HOME=/data' >&2
  exit 1
fi
# Keep the builder directory from being replaced by the runtime user. Existing
# runtime directories retain their contents, including the persistent CLI login.
chown root:root /data
chmod 711 /data
for name in home repo target cargo rustup worktrees runs merge repro-check bootstrap; do
  dir="/data/$name"
  if [ -L "$dir" ]; then
    echo "atb2: refusing symlink at $dir" >&2
    exit 1
  fi
  mkdir -p "$dir"
  chmod 700 "$dir"
  if [ "$name" = bootstrap ]; then
    chown builder:builder "$dir"
  else
    chown atb2:atb2 "$dir"
  fi
done

# Different UID, no supplementary groups/capabilities, no privilege escalation,
# and no inherited tokens. The root parent retains Infisical's machine token.
as_builder=(setpriv --reuid=1001 --regid=1001 --clear-groups
  --no-new-privs --bounding-set=-all env -i
  PATH="$PATH" HOME=/data/bootstrap/home USER=builder LOGNAME=builder
  LANG=C.UTF-8 TERM=dumb ATB2_HOME=/data/bootstrap
  CARGO_HOME=/data/bootstrap/cargo RUSTUP_HOME=/data/bootstrap/rustup)
# Seed private toolchains from the immutable image, once per volume. Rustup can
# then install the canary tree's required components without modifying shared
# executables or the other user's toolchain. Interrupted copies retry on boot.
# shellcheck disable=SC2016
seed_toolchain='
  if [ ! -f "$RUSTUP_HOME/.atb2-seeded" ]; then
    mkdir -p "$RUSTUP_HOME"
    cp -R /usr/local/rustup/. "$RUSTUP_HOME/"
    touch "$RUSTUP_HOME/.atb2-seeded"
  fi
'
"${as_builder[@]}" sh -eu -c "$seed_toolchain"
"${as_builder[@]}" ATB2_CANARY_REV="${ATB2_CANARY_REV:-}" /usr/local/bin/atb2-build-cli

as_runtime=(setpriv --reuid=1000 --regid=1000 --clear-groups
  --no-new-privs --bounding-set=-all)
"${as_runtime[@]}" env -i PATH="$PATH" RUSTUP_HOME=/data/rustup sh -eu -c "$seed_toolchain"
# Read the artifact as the builder, not root: a malicious artifact symlink
# cannot trick root into copying a credential. Write it as the runtime user.
# shellcheck disable=SC2016
artifact=$("${as_runtime[@]}" env -i PATH="$PATH" sh -eu -c '
    mkdir -p /data/target/debug
    mktemp /data/target/debug/.baml-cli.XXXXXX
  ')
# Only publish after BOTH the reader and writer succeed. pipefail alone is
# insufficient if the writer replaces the working binary before the reader exits.
# shellcheck disable=SC2016
if "${as_builder[@]}" cat /data/bootstrap/target/debug/baml-cli |
  "${as_runtime[@]}" env -i PATH="$PATH" sh -eu -c 'cat > "$1"' atb2 "$artifact"; then
  # shellcheck disable=SC2016
  if "${as_runtime[@]}" env -i PATH="$PATH" sh -eu -c '
      test -s "$1"
      chmod 700 "$1"
      mv -fT "$1" /data/target/debug/baml-cli
    ' atb2 "$artifact"; then
    artifact=""
  fi
fi
if [ -n "$artifact" ]; then
  "${as_runtime[@]}" rm -f -- "$artifact"
  echo 'atb2: compiler installation failed; previous runtime binary preserved' >&2
  exit 1
fi
unset ATB2_SECRETS_LOADED
export HOME=/data/home USER=atb2 LOGNAME=atb2
export CARGO_HOME=/data/cargo RUSTUP_HOME=/data/rustup
exec "${as_runtime[@]}" /usr/local/bin/atb2-entrypoint "$@"
