#!/usr/bin/env bash
# Runner entrypoint: pull the pipeline's secrets from Infisical, make sure
# canary's baml-cli exists on the volume, then run the BAML expression given
# as the command (runner_loop(), pipeline_loop()).
#
# Secrets: the machine holds ONE Fly secret, INFISICAL_TOKEN (a machine
# identity or service token for the boundary-tools project). Everything the
# pipeline reads (FEEDBACK_SUPABASE_*, ATB_SLACK_*, ATB_POSTHOG_*, GH_TOKEN,
# CLAUDE_CODE_OAUTH_TOKEN) is injected by `infisical run` at start, so a
# rotation in Infisical takes effect on the next restart and nothing is
# copied into Fly. INFISICAL_PROJECT_ID and INFISICAL_ENV come from fly.toml.
#
# The package is written against canary's BAML, so the released wrapper cannot
# run it; handle_issue builds canary's baml-cli into $ATB2_HOME/target on every
# run, and this does the same once up front so the first request does not
# pay for the build.
set -euo pipefail

if [ -n "${INFISICAL_TOKEN:-}" ] && [ -z "${ATB2_SECRETS_LOADED:-}" ]; then
  echo "atb2: secrets from Infisical (${INFISICAL_PROJECT_ID:?set in fly.toml}, ${INFISICAL_ENV:-prod})"
  export ATB2_SECRETS_LOADED=1
  exec infisical run \
    --projectId "$INFISICAL_PROJECT_ID" --env "${INFISICAL_ENV:-prod}" \
    --silent -- "$0" "$@"
fi

home="${ATB2_HOME:-/data}"
repo="$home/repo"
cli="$home/target/debug/baml-cli"

# gh + git use the token the way handle_issue expects (sandbox_env passes GH_TOKEN
# to gh; git pushes go through gh's credential helper)
# the old bot's token serves as GH_TOKEN when no GH_TOKEN is set
export GH_TOKEN="${GH_TOKEN:-${ATB_GITHUB_TOKEN:-}}"
if [ -n "${GH_TOKEN:-}" ]; then
  git config --global credential.helper '!gh auth git-credential'
  git config --global user.name "${ATB2_GIT_USER:-atb2}"
  git config --global user.email "${ATB2_GIT_EMAIL:-atb2@boundaryml.com}"
fi

if [ ! -x "$cli" ]; then
  echo "atb2: building canary's baml-cli into $home/target (first boot)"
  if [ ! -d "$repo/.git" ]; then
    git clone --branch canary https://github.com/BoundaryML/baml.git "$repo"
  fi
  (cd "$repo" && git fetch -q origin canary && git checkout -q canary && git merge -q --ff-only origin/canary)
  (cd "$repo/baml_language" && CARGO_TARGET_DIR="$home/target" cargo build -p baml_cli --bin baml-cli)
fi

expr="${1:-runner_loop()}"
echo "atb2: $expr"
cd /app
exec "$cli" run -e "$expr"
