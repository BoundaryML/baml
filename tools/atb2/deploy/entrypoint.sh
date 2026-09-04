#!/usr/bin/env bash
# Runtime only: bootstrap drops privileges before invoking this script.
set -euo pipefail
umask 077
if [ "$(id -u)" != 1000 ]; then
  echo "atb2: runtime must run as the atb2 user" >&2
  exit 1
fi
runner_home="${ATB2_HOME:-/data}"
export HOME="$runner_home/home"
cli="$runner_home/target/debug/baml-cli"
if [ -z "${ATB2_SECRETS_LOADED:-}" ]; then
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
