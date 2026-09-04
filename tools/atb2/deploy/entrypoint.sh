#!/usr/bin/env bash
# Runner entrypoint, in two phases.
#
# Phase 1 (no secrets in the environment): make sure canary's baml-cli on the
# volume matches the canary revision the runner should be on, building it
# with an explicit, allowlisted environment. Cargo runs build scripts and
# proc macros from the fetched tree, so this happens BEFORE any credential
# is loaded. ATB2_CANARY_REV pins the revision (a commit sha); unset, the
# runner tracks origin/canary, which the package is written against.
#
# Phase 2: re-exec under `infisical run` so the pipeline's secrets
# (FEEDBACK_SUPABASE_*, ATB_SLACK_*, ATB_POSTHOG_*, ATB_GITHUB_TOKEN) are
# present, set up gh, and run the BAML expression
# given as the command (runner_loop()). The machine holds one Fly secret,
# INFISICAL_TOKEN; INFISICAL_PROJECT_ID and INFISICAL_ENV come from fly.toml.
set -euo pipefail

runner_home="${ATB2_HOME:-/data}"
# the CLI login (claude) lives under HOME; keep it on the volume so it
# survives redeploys. atb2 never handles a Claude credential itself.
export HOME="$runner_home/home"
mkdir -p "$HOME"
repo="$runner_home/repo"
cli="$runner_home/target/debug/baml-cli"
built="$runner_home/target/.baml-cli-rev"

if [ -z "${ATB2_SECRETS_LOADED:-}" ]; then
  # ---- phase 1: the toolchain, with no secrets around
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
    (cd "$repo" && git checkout -q --detach "$want")
    # an explicit environment: nothing from this process reaches cargo
    (cd "$repo/baml_language" && env -i \
        PATH="$PATH" HOME="$HOME" USER="${USER:-atb2}" LANG=C.UTF-8 TERM=dumb \
        CARGO_HOME="${CARGO_HOME:-/usr/local/cargo}" RUSTUP_HOME="${RUSTUP_HOME:-/usr/local/rustup}" \
        CARGO_TARGET_DIR="$runner_home/target" CARGO_INCREMENTAL=0 \
        cargo build -p baml_cli --bin baml-cli)
    echo "$want" > "$built"
  fi
  # ---- phase 2: secrets, then the pipeline
  if [ -n "${INFISICAL_TOKEN:-}" ]; then
    echo "atb2: secrets from Infisical (${INFISICAL_PROJECT_ID:?set in fly.toml}, ${INFISICAL_ENV:-prod})"
    export ATB2_SECRETS_LOADED=1
    exec infisical run \
      --projectId "$INFISICAL_PROJECT_ID" --env "${INFISICAL_ENV:-prod}" \
      --silent -- "$0" "$@"
  fi
  export ATB2_SECRETS_LOADED=1
fi

# gh + git use the token the way handle_issue expects (sandbox_env passes GH_TOKEN
# to gh; git pushes go through gh's credential helper); the old bot's token
# serves as GH_TOKEN when no GH_TOKEN is set
export GH_TOKEN="${GH_TOKEN:-${ATB_GITHUB_TOKEN:-}}"
if [ -n "${GH_TOKEN:-}" ]; then
  git config --global credential.helper '!gh auth git-credential'
  git config --global user.name "${ATB2_GIT_USER:-atb2}"
  git config --global user.email "${ATB2_GIT_EMAIL:-atb2@boundaryml.com}"
fi

expr="${1:-runner_loop()}"
echo "atb2: $expr"
cd /app
exec "$cli" run -e "$expr"
