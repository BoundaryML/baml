# Releasing the engine

Engine releases are triggered by pushing a versioned `<version>-engine` source tag. The trigger tag must point to the exact tested `canary` commit intended for release.

## Prepare the release

1. Set the intended version and run the bump script from a clean checkout: `VERSION=0.226.2; ./tools/bump-version.py --all --new-version "$VERSION"`. Follow its prompts, review the generated version and changelog changes, then merge the resulting PR into `canary`.
2. Wait for the `canary` CI run for the exact commit intended for release to pass.
3. Confirm that neither the trigger tag nor either release tag exists: `git ls-remote --tags origin refs/tags/0.226.2-engine refs/tags/0.226.2 refs/tags/v0.226.2` should print nothing (replace `0.226.2` with the intended version).

The workflow derives the release version from `current_version` in `tools/versions/engine.cfg`. It requires the triggering ref to be exactly `refs/tags/<version>-engine`, then verifies that every other `tools/versions/*.cfg` file and the changelog agree with that version.

The release workflow runs `integ-tests` for informational signal, but its failures are tolerated and it is deliberately excluded from the release gate because it is currently too flaky. The green `canary` CI run is the operator's test prerequisite.

## Run the release

1. Fetch `canary`, record its commit, and confirm that it is the exact green commit intended for release: `git fetch origin canary && git rev-parse origin/canary`.
2. Create the trigger tag at that commit: `VERSION=0.226.2; git tag "$VERSION-engine" origin/canary`.
3. Push only the trigger tag: `git push origin "refs/tags/$VERSION-engine"`.
4. Open **Actions → BAML Release**, confirm the trigger tag, derived version, and commit in the workflow summary, and approve the `release` environment when requested.

The workflow builds all release artifacts from the trigger tag's commit before making external changes. The `create-release-tag` job then creates annotated `<version>` and `v<version>` tags through the GitHub API, verifying each tag before proceeding. Publishing jobs, including the GitHub Release, run only after both tags exist; this prevents the GitHub Release API from implicitly creating a lightweight tag.

## Recover from a transient failure

Use **Re-run failed jobs** on the same workflow run. Do not move, recreate, or push another trigger tag for the same version. The workflow rejects a new run when either release tag already exists. Release-tag creation is idempotent only within the original run: a retry verifies that both release tags are annotated, point to the original workflow commit, and contain the original run URL.

Never delete, move, force-push, or recreate a trigger or published release tag. If product or workflow source must change after the trigger tag was pushed, fix the problem and release a new patch version.

After the run succeeds, verify both tags and the GitHub Release:

```bash
VERSION=0.226.2
git ls-remote --tags origin "refs/tags/$VERSION" "refs/tags/$VERSION^{}" "refs/tags/v$VERSION" "refs/tags/v$VERSION^{}"
gh release view "$VERSION"
```
