#!/usr/bin/env bash
# Runtime only: bootstrap drops privileges before invoking this script.
set -euo pipefail
umask 077
if [ "$(id -u)" != 1000 ] || [ -n "${INFISICAL_TOKEN+x}" ]; then
  echo "atb2: runtime requires the atb2 user without an Infisical machine token" >&2
  exit 1
fi
runner_home="${ATB2_HOME:-/data}"
export HOME="$runner_home/home"
cli="$runner_home/target/debug/baml-cli"

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
