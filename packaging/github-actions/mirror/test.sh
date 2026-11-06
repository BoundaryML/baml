#!/usr/bin/env bash
# Tests sync.sh against local bare repositories standing in for this
# repository and the mirrors. Each sync runs from a shallow checkout of one
# source commit, as the sync workflow does.
set -euo pipefail

here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
actions="$(cd "$here/.." && pwd)"
root="$(cd "$actions/../.." && pwd)"
sync="$here/sync.sh"
work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT
export GIT_CONFIG_GLOBAL="$work/gitconfig" GIT_CONFIG_NOSYSTEM=1
git config --global init.defaultBranch main
git config --global user.name test
git config --global user.email test@example.com
git config --global advice.detachedHead false

fail() {
  echo "FAIL: $*" >&2
  exit 1
}

# Upstream source repository: commits A <- B <- C on main, with the real
# actions and LICENSE, and E on a side branch from C.
upstream="$work/source.git"
git init --quiet --bare "$upstream"
git -C "$upstream" config uploadpack.allowFilter true
git -C "$upstream" config uploadpack.allowAnySHA1InWant true
dev="$work/source-dev"
git clone --quiet "$upstream" "$dev" 2>/dev/null
mkdir -p "$dev/packaging/github-actions"
cp "$root/LICENSE" "$dev/LICENSE"
cp -R "$actions/setup-baml" "$actions/baml-action" "$dev/packaging/github-actions/"
rm -rf "$dev/packaging/github-actions/"*/node_modules
commit() {
  git -C "$dev" add --all
  git -C "$dev" commit --quiet -m "$1"
  git -C "$dev" rev-parse HEAD
}
commit A >/dev/null
# Ignored by setup-baml's .gitignore, but force-added, so it must be published.
echo forced >"$dev/packaging/github-actions/setup-baml/debug.log"
git -C "$dev" add --force packaging/github-actions/setup-baml/debug.log
B="$(commit B)"
echo "change" >>"$dev/packaging/github-actions/setup-baml/README.md"
C="$(commit C)"
git -C "$dev" push --quiet origin main
git -C "$dev" switch --quiet -c side
mkdir -p "$dev/packaging/github-actions/baml-action/nested"
printf 'runs:\n  using: composite\n  steps:\n    - uses: "./elsewhere"\n' >"$dev/packaging/github-actions/baml-action/nested/action.yml"
E="$(commit E)"
git -C "$dev" push --quiet origin side

# A shallow checkout of one source commit, like actions/checkout.
checkout() {
  local dir="$work/checkout-$1"
  rm -rf "$dir"
  git init --quiet "$dir"
  git -C "$dir" remote add origin "file://$upstream"
  git -C "$dir" fetch --quiet --depth 1 --filter=blob:none origin "$1"
  git -C "$dir" checkout --quiet FETCH_HEAD
  echo "untracked" >"$dir/packaging/github-actions/setup-baml/untracked.txt"
  echo "$dir"
}

# run <source-commit> <action> <mirror> [setup-baml-sha]
run() {
  local source="$1" action="$2" mirror="$3"
  SYNC_SOURCE_REPO="$(checkout "$source")" \
    "$sync" "$action" "file://$work/$mirror.git" v1 "$source" ${4:+"$4"}
}

# A mirror with one hand-made commit, like the public repos before a sync.
new_mirror() {
  git init --quiet --bare "$work/$1.git"
  git clone --quiet "$work/$1.git" "$work/seed-$1" 2>/dev/null
  echo "old" >"$work/seed-$1/README.md"
  git -C "$work/seed-$1" add README.md
  git -C "$work/seed-$1" commit --quiet -m "old content"
  git -C "$work/seed-$1" push --quiet origin main
}

rev() { git --git-dir "$work/$1.git" rev-parse "$2"; }
show() { git --git-dir "$work/$1.git" show "$2:$3"; }
count() { git --git-dir "$work/$1.git" rev-list --count main; }

new_mirror setup-baml
new_mirror baml-action

# setup-baml publishes the committed tree and points v1 at it.
setup_b="$(run "$B" setup-baml setup-baml 2>/dev/null)"
[[ "$setup_b" =~ ^[0-9a-f]{40}$ ]] || fail "setup-baml sync printed '$setup_b'"
[[ "$(rev setup-baml main)" == "$setup_b" ]] || fail "setup-baml main is not the printed commit"
[[ "$(rev setup-baml v1)" == "$setup_b" ]] || fail "setup-baml v1 is not the printed commit"
show setup-baml "$setup_b" action.yml | grep -q '^name: Setup BAML$' || fail "action.yml not published"
show setup-baml "$setup_b" LICENSE | grep -q 'Apache License' || fail "root LICENSE not published"
[[ "$(show setup-baml "$setup_b" debug.log)" == forced ]] || fail "force-added ignored file not published"
if show setup-baml "$setup_b" untracked.txt >/dev/null 2>&1; then fail "untracked file published"; fi
if show setup-baml "$setup_b" README.md | grep -qx old; then fail "stale mirror file survived"; fi
[[ "$(git --git-dir "$work/setup-baml.git" log -1 --format=%s main)" == "Sync from BoundaryML/baml@$B" ]] || fail "unexpected commit message"

# The same source again commits nothing and prints the same commit.
[[ "$(run "$B" setup-baml setup-baml 2>/dev/null)" == "$setup_b" ]] || fail "unchanged sync moved HEAD"
[[ "$(count setup-baml)" == 2 ]] || fail "unchanged sync created a commit"

# A v1 left behind (e.g. a failed earlier push) is moved to the matching HEAD.
git --git-dir "$work/setup-baml.git" tag --force v1 "$setup_b~1" >/dev/null
run "$B" setup-baml setup-baml >/dev/null 2>&1
[[ "$(rev setup-baml v1)" == "$setup_b" ]] || fail "stale v1 was not repaired"

# baml-action pins the setup-baml commit that was just published.
action_b="$(run "$B" baml-action baml-action "$setup_b" 2>/dev/null)"
[[ "$(rev baml-action v1)" == "$action_b" ]] || fail "baml-action v1 is not the printed commit"
published="$(show baml-action "$action_b" action.yml)"
grep -qxF "      uses: BoundaryML/setup-baml@$setup_b # v1" <<<"$published" || fail "baml-action does not pin setup-baml@$setup_b"
if grep -q 'uses: \./' <<<"$published"; then fail "baml-action still uses a local path"; fi
run "$B" baml-action baml-action "$setup_b" >/dev/null 2>&1
[[ "$(count baml-action)" == 2 ]] || fail "unchanged baml-action sync created a commit"

# A newer source commit publishes both, and the pin moves.
setup_c="$(run "$C" setup-baml setup-baml 2>/dev/null)"
[[ "$setup_c" != "$setup_b" ]] || fail "C did not republish setup-baml"
action_c="$(run "$C" baml-action baml-action "$setup_c" 2>/dev/null)"
show baml-action "$action_c" action.yml | grep -qxF "      uses: BoundaryML/setup-baml@$setup_c # v1" || fail "pin did not move"

# Rerunning an older sync (B, after C was published) is refused and changes
# nothing: not the branch, not v1.
for args in "setup-baml setup-baml" "baml-action baml-action $setup_b"; do
  # shellcheck disable=SC2086
  if run "$B" $args >"$work/out" 2>&1; then fail "older source B republished: $args"; fi
  grep -q "already synced from $C, which is newer than $B" "$work/out" || fail "unexpected refusal: $(cat "$work/out")"
done
[[ "$(rev setup-baml main) $(rev setup-baml v1)" == "$setup_c $setup_c" ]] || fail "refused setup-baml sync moved the mirror"
[[ "$(rev baml-action main) $(rev baml-action v1)" == "$action_c $action_c" ]] || fail "refused baml-action sync moved the mirror"

# Refusals before touching the mirror: bad arguments, and a nested
# action.yml that uses a quoted local path.
for args in "$C baml-action baml-action" "$C baml-action baml-action main" "$E baml-action baml-action $setup_c"; do
  # shellcheck disable=SC2086
  if run $args >/dev/null 2>&1; then fail "sync.sh accepted: $args"; fi
done
if SYNC_SOURCE_REPO="$(checkout "$C")" "$sync" setup-baml "file://$work/setup-baml.git" latest "$C" >/dev/null 2>&1; then
  fail "sync.sh accepted major tag 'latest'"
fi
[[ "$(rev baml-action main) $(rev baml-action v1)" == "$action_c $action_c" ]] || fail "a refused sync changed the mirror"

# A push whose branch update is rejected moves neither the branch nor the
# tag, even though the tag alone would have been accepted.
new_mirror rejecting
run "$B" setup-baml rejecting >/dev/null 2>&1
before="$(rev rejecting main) $(rev rejecting v1)"
# shellcheck disable=SC2016 # $1 belongs to the hook.
printf '#!/bin/sh\ntest "$1" != refs/heads/main\n' >"$work/rejecting.git/hooks/update"
chmod +x "$work/rejecting.git/hooks/update"
if run "$C" setup-baml rejecting >/dev/null 2>&1; then fail "sync.sh succeeded although the push was rejected"; fi
[[ "$(rev rejecting main) $(rev rejecting v1)" == "$before" ]] || fail "a rejected push moved the mirror"

echo "mirror sync tests passed"
