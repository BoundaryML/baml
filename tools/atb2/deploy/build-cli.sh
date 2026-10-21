#!/usr/bin/env bash
# Runs only as the isolated builder. Its home, source and cache contain no login.
set -euo pipefail
umask 077
runner_home="${ATB2_HOME:?builder cache required}"
export HOME="$runner_home/home"
mkdir -p "$HOME"
repo="$runner_home/repo"
cli="$runner_home/target/debug/baml-cli"
built="$runner_home/target/.baml-cli-rev"
have="$(cat "$built" 2>/dev/null || true)"
want="${ATB2_CANARY_REV:-}"
# A pinned, matching executable is sufficient for boot, even offline.
# Tracking canary and missing/mismatched builds still require a fresh fetch;
# never silently start a different revision from the one requested.
if [ -z "$want" ] || [ ! -x "$cli" ] || [ "$have" != "$want" ]; then
  if [ ! -d "$repo/.git" ]; then
    git clone --branch canary https://github.com/BoundaryML/baml.git "$repo"
  fi
  (cd "$repo" && git fetch -q origin canary)
  want="${ATB2_CANARY_REV:-$(cd "$repo" && git rev-parse origin/canary)}"
fi
if [ ! -x "$cli" ] || [ "$have" != "$want" ]; then
  echo "atb2: building baml-cli at $want into $runner_home/target (had: ${have:-none})"
  # A failed build may already have changed the artifact. It must not retain
  # the old revision marker and pass a later pinned-cache check.
  rm -f -- "$built"
  (cd "$repo" && git checkout -q --detach "$want")
  # an explicit environment: nothing from this process reaches cargo
  (cd "$repo/baml_language" && env -i \
      PATH="$PATH" HOME="$HOME" USER="${USER:-atb2}" LANG=C.UTF-8 TERM=dumb \
      CARGO_HOME="${CARGO_HOME:-/usr/local/cargo}" RUSTUP_HOME="${RUSTUP_HOME:-/usr/local/rustup}" \
      CARGO_TARGET_DIR="$runner_home/target" CARGO_INCREMENTAL=0 \
      cargo build -p baml_cli --bin baml-cli)
  echo "$want" > "$built"
fi
