#!/usr/bin/env bash
# Runs only as the isolated builder. Its home, source and cache contain no login.
#
# Which toolchain gets built:
#   ATB2_CANARY_REV=<sha>     that exact canary commit (a pin; boots offline
#                             when the cached build already matches)
#   ATB2_TOOLCHAIN=canary     origin/canary's head
#   otherwise                 the latest published nightly, from the package
#                             host's nightly manifest, checked out by its tag.
#                             Every repro the pipeline runs is then confirmed
#                             on the same nightly reporters can install.
set -euo pipefail
umask 077
runner_home="${ATB2_HOME:?builder cache required}"
export HOME="$runner_home/home"
mkdir -p "$HOME"
repo="$runner_home/repo"
cli="$runner_home/target/debug/baml-cli"
built="$runner_home/target/.baml-cli-rev"
built_version="$runner_home/target/.baml-cli-version"
manifest="${ATB2_NIGHTLY_MANIFEST:-https://pkg.boundaryml.com/manifest/v1/nightly.json}"
have="$(cat "$built" 2>/dev/null || true)"
want="${ATB2_CANARY_REV:-}"
version=""
# A pinned, matching executable is sufficient for boot, even offline.
# Tracking canary or the nightly and missing/mismatched builds still require
# a fresh fetch; never silently start a different revision from the one requested.
if [ -z "$want" ] || [ ! -x "$cli" ] || [ "$have" != "$want" ]; then
  if [ ! -d "$repo/.git" ]; then
    git clone --branch canary https://github.com/BoundaryML/baml.git "$repo"
  fi
  if [ -n "${ATB2_CANARY_REV:-}" ] || [ "${ATB2_TOOLCHAIN:-nightly}" = "canary" ]; then
    (cd "$repo" && git fetch -q origin canary)
    want="${ATB2_CANARY_REV:-$(cd "$repo" && git rev-parse origin/canary)}"
  else
    # The manifest names the nightly the `baml` wrapper installs; the tag
    # carries the same version, so `baml-cli --version` matches it.
    version="$(curl -fsS --max-time 30 "$manifest" | python3 -I -c '
import json, re, sys
version = json.load(sys.stdin).get("version", "")
if not re.fullmatch(r"[0-9]+\.[0-9]+\.[0-9]+-nightly\.[0-9]{8}\.[a-z]", version):
    raise SystemExit("atb2: nightly manifest names no nightly version")
print(version)
')"
    tag="baml-language-$version"
    (cd "$repo" && git fetch -q --no-tags origin "refs/tags/$tag")
    want="$(cd "$repo" && git rev-parse 'FETCH_HEAD^{commit}')"
  fi
fi
if [ ! -x "$cli" ] || [ "$have" != "$want" ]; then
  echo "atb2: building baml-cli at $want${version:+ (nightly $version)} into $runner_home/target (had: ${have:-none})"
  # A failed build may already have changed the artifact. It must not retain
  # the old revision marker and pass a later pinned-cache check.
  rm -f -- "$built" "$built_version"
  (cd "$repo" && git checkout -q --detach "$want")
  # an explicit environment: nothing from this process reaches cargo
  (cd "$repo/baml_language" && env -i \
      PATH="$PATH" HOME="$HOME" USER="${USER:-atb2}" LANG=C.UTF-8 TERM=dumb \
      CARGO_HOME="${CARGO_HOME:-/usr/local/cargo}" RUSTUP_HOME="${RUSTUP_HOME:-/usr/local/rustup}" \
      CARGO_TARGET_DIR="$runner_home/target" CARGO_INCREMENTAL=0 \
      cargo build -p baml_cli --bin baml-cli)
  echo "$want" > "$built"
  if [ -n "$version" ]; then
    echo "$version" > "$built_version"
  fi
fi
