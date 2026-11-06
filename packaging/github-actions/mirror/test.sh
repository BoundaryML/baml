#!/usr/bin/env bash
# Tests sync.sh against local bare repositories standing in for the mirrors.
set -euo pipefail

here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
sync="$here/sync.sh"
work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT
export GIT_CONFIG_GLOBAL="$work/gitconfig" GIT_CONFIG_NOSYSTEM=1
git config --global init.defaultBranch main
git config --global user.name test
git config --global user.email test@example.com

fail() {
  echo "FAIL: $*" >&2
  exit 1
}

# A mirror with one commit, like the public repos before a sync.
new_mirror() {
  local name="$1"
  git init --quiet --bare "$work/$name.git"
  git clone --quiet "$work/$name.git" "$work/seed-$name" 2>/dev/null
  echo "old" >"$work/seed-$name/README.md"
  git -C "$work/seed-$name" add README.md
  git -C "$work/seed-$name" commit --quiet -m "old content"
  git -C "$work/seed-$name" push --quiet origin main
  echo "file://$work/$name.git"
}

show() {
  git --git-dir "$work/$1.git" show "$2:$3"
}

source_sha=0123456789abcdef0123456789abcdef01234567
setup_url="$(new_mirror setup-baml)"
action_url="$(new_mirror baml-action)"

# setup-baml publishes the source tree and points v1 at it.
setup_sha="$("$sync" setup-baml "$setup_url" v1 "$source_sha" 2>/dev/null)"
[[ "$setup_sha" =~ ^[0-9a-f]{40}$ ]] || fail "setup-baml sync printed '$setup_sha'"
[[ "$(git --git-dir "$work/setup-baml.git" rev-parse main)" == "$setup_sha" ]] || fail "setup-baml main is not the printed commit"
[[ "$(git --git-dir "$work/setup-baml.git" rev-parse v1)" == "$setup_sha" ]] || fail "setup-baml v1 is not the printed commit"
show setup-baml "$setup_sha" action.yml | grep -q '^name: Setup BAML$' || fail "setup-baml action.yml not published"
show setup-baml "$setup_sha" LICENSE | grep -q 'Apache License' || fail "root LICENSE not published"
if show setup-baml "$setup_sha" README.md | grep -qx old; then fail "stale mirror file survived"; fi
[[ "$(git --git-dir "$work/setup-baml.git" log -1 --format=%s main)" == "Sync from BoundaryML/baml@$source_sha" ]] || fail "unexpected commit message"

# Re-running with unchanged content commits nothing and prints the same commit.
again="$("$sync" setup-baml "$setup_url" v1 "$source_sha" 2>/dev/null)"
[[ "$again" == "$setup_sha" ]] || fail "unchanged setup-baml sync moved to $again"
[[ "$(git --git-dir "$work/setup-baml.git" rev-list --count main)" == 2 ]] || fail "unchanged sync created a commit"

# A v1 left behind (e.g. a failed earlier push) is moved to the matching HEAD.
git --git-dir "$work/setup-baml.git" tag --force v1 "$setup_sha~1" >/dev/null
"$sync" setup-baml "$setup_url" v1 "$source_sha" >/dev/null 2>&1
[[ "$(git --git-dir "$work/setup-baml.git" rev-parse v1)" == "$setup_sha" ]] || fail "stale v1 was not repaired"

# baml-action pins the setup-baml commit that was just published.
action_sha="$("$sync" baml-action "$action_url" v1 "$source_sha" "$setup_sha" 2>/dev/null)"
[[ "$(git --git-dir "$work/baml-action.git" rev-parse v1)" == "$action_sha" ]] || fail "baml-action v1 is not the printed commit"
published="$(show baml-action "$action_sha" action.yml)"
grep -qxF "      uses: BoundaryML/setup-baml@$setup_sha # v1" <<<"$published" || fail "baml-action does not pin setup-baml@$setup_sha"
if grep -q 'uses: \./' <<<"$published"; then fail "baml-action still uses a local path"; fi

# Same inputs: no new commit. A new setup-baml commit: the pin moves.
"$sync" baml-action "$action_url" v1 "$source_sha" "$setup_sha" >/dev/null 2>&1
[[ "$(git --git-dir "$work/baml-action.git" rev-list --count main)" == 2 ]] || fail "unchanged baml-action sync created a commit"
next_setup=fedcba9876543210fedcba9876543210fedcba98
next_action="$("$sync" baml-action "$action_url" v1 "$source_sha" "$next_setup" 2>/dev/null)"
[[ "$next_action" != "$action_sha" ]] || fail "a new setup-baml commit did not republish baml-action"
show baml-action "$next_action" action.yml | grep -qxF "      uses: BoundaryML/setup-baml@$next_setup # v1" || fail "pin did not move"

# Refusals: no setup-baml commit, a malformed one, a bad major tag.
for args in "baml-action $action_url v1 $source_sha" \
  "baml-action $action_url v1 $source_sha main" \
  "setup-baml $setup_url latest $source_sha"; do
  # shellcheck disable=SC2086
  if "$sync" $args >/dev/null 2>&1; then fail "sync.sh accepted: $args"; fi
done
[[ "$(git --git-dir "$work/baml-action.git" rev-parse main)" == "$next_action" ]] || fail "a refused sync changed the mirror"

echo "mirror sync tests passed"
