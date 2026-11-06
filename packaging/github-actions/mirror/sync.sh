#!/usr/bin/env bash
# Publishes one action from packaging/github-actions to its read-only mirror.
#
#   sync.sh setup-baml  <mirror-url> <major> <source-sha>
#   sync.sh baml-action <mirror-url> <major> <source-sha> <setup-baml-sha>
#
# Copies the action and the root LICENSE into a clone of the mirror. If that
# changes the mirror, commits and pushes it. Then points <major> at the mirror's
# HEAD. Prints the mirror's HEAD commit, whose tree is exactly what this run
# published.
#
# In this repository baml-action uses the setup-baml next to it, so CI tests
# them together. The published baml-action can't reach that path, so its
# `uses:` is rewritten to <setup-baml-sha>: the setup-baml mirror commit
# published from the same source commit.
set -euo pipefail

action="${1:?usage: sync.sh <action> <mirror-url> <major> <source-sha> [setup-baml-sha]}"
mirror_url="${2:?missing mirror URL}"
major="${3:?missing major tag}"
source_sha="${4:?missing source commit}"
setup_baml_sha="${5:-}"

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
source_dir="$root/packaging/github-actions/$action"
readonly local_setup_baml="uses: ./packaging/github-actions/setup-baml"

if [[ ! -f "$source_dir/action.yml" ]]; then
  echo "sync.sh: no action at $source_dir" >&2
  exit 1
fi
if [[ ! "$major" =~ ^v[0-9]+$ ]]; then
  echo "sync.sh: major tag must look like v1, got '$major'" >&2
  exit 1
fi

work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT
mirror="$work/mirror"

git clone --quiet --depth 50 "$mirror_url" "$mirror" >&2
rsync -a --delete --exclude .git "$source_dir/" "$mirror/"
cp "$root/LICENSE" "$mirror/LICENSE"

if [[ "$action" == "baml-action" ]]; then
  if [[ ! "$setup_baml_sha" =~ ^[0-9a-f]{40}$ ]]; then
    echo "sync.sh: baml-action needs the published setup-baml commit, got '$setup_baml_sha'" >&2
    exit 1
  fi
  if [[ "$(grep -cxF "      $local_setup_baml" "$mirror/action.yml")" != 1 ]]; then
    echo "sync.sh: baml-action/action.yml must use the local setup-baml exactly once" >&2
    exit 1
  fi
  sed -i.bak "s|^      $local_setup_baml\$|      uses: BoundaryML/setup-baml@$setup_baml_sha # $major|" "$mirror/action.yml"
  rm "$mirror/action.yml.bak"
fi
# A published action can only use actions by repository; a local path would
# resolve against the user's workspace.
if grep -nE '^\s*(- )?uses:\s*\./' "$mirror/action.yml" >&2; then
  echo "sync.sh: the published $action would use a local path" >&2
  exit 1
fi

if [[ -n "$(git -C "$mirror" status --porcelain)" ]]; then
  git -C "$mirror" status --porcelain >&2
  git -C "$mirror" add -A
  git -C "$mirror" \
    -c user.name="github-actions[bot]" \
    -c user.email="41898282+github-actions[bot]@users.noreply.github.com" \
    commit --quiet -m "Sync from BoundaryML/baml@$source_sha"
else
  echo "$action mirror already matches BoundaryML/baml@$source_sha" >&2
fi

head="$(git -C "$mirror" rev-parse HEAD)"
tagged="$(git -C "$mirror" ls-remote --tags origin "refs/tags/$major" | cut -f1)"
if [[ "$head" != "$(git -C "$mirror" rev-parse '@{upstream}')" || "$tagged" != "$head" ]]; then
  git -C "$mirror" tag --force "$major" "$head" >&2
  # --atomic: the branch never advances without its tag, or the next run would
  # see no content diff and leave the tag behind.
  git -C "$mirror" push --quiet --atomic origin HEAD "+refs/tags/$major" >&2
fi
echo "$head"
