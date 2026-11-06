#!/usr/bin/env bash
# Publishes one action from packaging/github-actions to its read-only mirror.
#
#   sync.sh setup-baml  <mirror-url> <major> <source-sha>
#   sync.sh baml-action <mirror-url> <major> <source-sha> <setup-baml-sha>
#
# Replaces the mirror's files with the action's committed tree at <source-sha>
# and the root LICENSE. If that changes the mirror, commits and pushes it. Then
# points <major> at the mirror's HEAD. Prints the mirror's HEAD commit, whose
# tree is exactly what this run published.
#
# In this repository baml-action uses the setup-baml next to it, so CI tests
# them together. The published baml-action can't reach that path, so its
# `uses:` is rewritten to <setup-baml-sha>: the setup-baml mirror commit
# published from the same source commit.
#
# Refuses to publish a <source-sha> that is an ancestor of the commit the
# mirror was last synced from, such as a rerun of an older sync.
#
# The source repository is this checkout. Tests point SYNC_SOURCE_REPO at
# another repository with the same layout.
set -euo pipefail

action="${1:?usage: sync.sh <action> <mirror-url> <major> <source-sha> [setup-baml-sha]}"
mirror_url="${2:?missing mirror URL}"
major="${3:?missing major tag}"
source_sha="${4:?missing source commit}"
setup_baml_sha="${5:-}"

source_repo="${SYNC_SOURCE_REPO:-$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)}"
action_path="packaging/github-actions/$action"
readonly local_setup_baml="      uses: ./packaging/github-actions/setup-baml"
readonly sync_subject="Sync from BoundaryML/baml@"

fail() {
  echo "sync.sh: $*" >&2
  exit 1
}

[[ "$source_sha" =~ ^[0-9a-f]{40}$ ]] || fail "source commit must be a full SHA, got '$source_sha'"
[[ "$major" =~ ^v[0-9]+$ ]] || fail "major tag must look like v1, got '$major'"
git -C "$source_repo" cat-file -e "$source_sha:$action_path/action.yml" 2>/dev/null ||
  fail "no action at $action_path in $source_sha"
if [[ "$action" == "baml-action" && ! "$setup_baml_sha" =~ ^[0-9a-f]{40}$ ]]; then
  fail "baml-action needs the published setup-baml commit, got '$setup_baml_sha'"
fi

work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT
mirror="$work/mirror"
git clone --quiet --depth 50 "$mirror_url" "$mirror" >&2

# Never move the mirror back to an older source commit. The rerun case: the
# checkout is shallow at <source-sha>, so fetch the newer published commit's
# history (commits only) and walk back from it.
published="$(git -C "$mirror" log -1 --format=%s --grep="^$sync_subject" |
  sed -n "s|^$sync_subject\([0-9a-f]\{40\}\)\$|\1|p")"
if [[ -n "$published" && "$published" != "$source_sha" ]]; then
  if ! git -C "$source_repo" cat-file -e "$published^{commit}" 2>/dev/null; then
    git -C "$source_repo" fetch --quiet --no-tags --filter=tree:0 origin "$published" >&2 ||
      fail "can't fetch $published, the commit $action was last synced from"
  fi
  if git -C "$source_repo" merge-base --is-ancestor "$source_sha" "$published"; then
    fail "$action was already synced from $published, which is newer than $source_sha"
  fi
fi

# Replace everything but .git with the committed tree.
find "$mirror" -mindepth 1 -maxdepth 1 ! -name .git -exec rm -rf {} +
git -C "$source_repo" archive "$source_sha:$action_path" | tar -x -C "$mirror"
git -C "$source_repo" show "$source_sha:LICENSE" >"$mirror/LICENSE"

if [[ "$action" == "baml-action" ]]; then
  [[ "$(grep -cxF "$local_setup_baml" "$mirror/action.yml")" == 1 ]] ||
    fail "baml-action/action.yml must use the local setup-baml exactly once"
  awk -v old="$local_setup_baml" -v new="      uses: BoundaryML/setup-baml@$setup_baml_sha # $major" \
    '$0 == old { print new; next } { print }' "$mirror/action.yml" >"$work/action.yml"
  mv "$work/action.yml" "$mirror/action.yml"
fi
# A published action can only use actions by repository; a local path would
# resolve against the user's workspace.
if grep -rnE --include=action.yml --include=action.yaml \
  "^[[:space:]]*(-[[:space:]]+)?uses:[[:space:]]*[\"']?\./" "$mirror" >&2; then
  fail "the published $action would use a local path"
fi

# --force: a file the action force-added past its .gitignore is still published.
git -C "$mirror" add --all --force
if ! git -C "$mirror" diff --cached --quiet; then
  git -C "$mirror" status --short >&2
  git -C "$mirror" \
    -c user.name="github-actions[bot]" \
    -c user.email="41898282+github-actions[bot]@users.noreply.github.com" \
    commit --quiet -m "$sync_subject$source_sha"
else
  echo "$action mirror already matches BoundaryML/baml@$source_sha" >&2
fi

head="$(git -C "$mirror" rev-parse HEAD)"
# The peeled line comes last for an annotated tag.
tagged="$(git -C "$mirror" ls-remote origin "refs/tags/$major" "refs/tags/$major^{}" | tail -n1 | cut -f1)"
if [[ "$head" != "$(git -C "$mirror" rev-parse '@{upstream}')" || "$tagged" != "$head" ]]; then
  git -C "$mirror" tag --force "$major" "$head" >&2
  # --atomic: the branch never advances without its tag, or the next run would
  # see no content diff and leave the tag behind.
  git -C "$mirror" push --quiet --atomic origin HEAD "+refs/tags/$major" >&2
fi
echo "$head"
