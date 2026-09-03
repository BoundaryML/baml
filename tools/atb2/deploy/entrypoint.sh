#!/usr/bin/env bash
# Runner entrypoint: make sure canary's baml-cli exists on the volume, then
# run the BAML expression given as the command (babysit_loop(), pipeline_loop()).
#
# The package is written against canary's BAML, so the released wrapper cannot
# run it; handle_issue builds canary's baml-cli into $ATB2_HOME/target on every
# run, and this does the same once up front so the first request does not
# pay for the build.
set -euo pipefail

home="${ATB2_HOME:-/data}"
repo="$home/repo"
cli="$home/target/debug/baml-cli"

# gh + git use the token the way handle_issue expects (sandbox_env passes GH_TOKEN
# to gh; git pushes go through gh's credential helper)
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

expr="${1:-babysit_loop()}"
echo "atb2: $expr"
cd /app
exec "$cli" run -e "$expr"
