# Releasing the engine

Engine releases are started manually from `canary`. The workflow is not triggered by tag pushes or path-filtered pushes because the set of files that affects an engine release is too broad to maintain safely.

## Prepare the release

1. Set the intended version and run the bump script from a clean checkout: `VERSION=0.226.2; ./tools/bump-version.py --all --new-version "$VERSION"`. Follow its prompts, review the generated version and changelog changes, then merge the resulting PR into `canary`.
2. Wait for the `canary` CI run for the merged commit to pass.
3. Confirm that neither release tag exists: `git ls-remote --tags origin refs/tags/0.226.2 refs/tags/v0.226.2` should print nothing (replace `0.226.2` with the intended version).

The workflow derives the release version from `current_version` in `tools/versions/engine.cfg`. It verifies that every other `tools/versions/*.cfg` file and the changelog agree with that version.

## Run the release

1. Open **Actions → BAML Release → Run workflow**.
2. Select the `canary` branch.
3. Leave `dry_run` unchecked for a real release. Check it only to build and verify artifacts without creating tags or publishing.
4. Start the workflow, confirm the derived version and commit in the workflow summary, and approve the `release` environment when requested.

The workflow builds all release artifacts from the selected `canary` commit before making external changes. The `create-release-tag` job then atomically pushes annotated `<version>` and `v<version>` tags. Publishing jobs, including the GitHub Release, run only after those tags exist; this prevents the GitHub Release API from implicitly creating a lightweight tag.

## Recover from a transient failure

Use **Re-run failed jobs** on the same workflow run. Do not start a new workflow run for the same version; the workflow rejects a new dispatch when either release tag already exists. Tag creation is idempotent only within the original run: a retry verifies that both tags are annotated, point to the original workflow commit, and contain the original run URL.

Never delete, move, force-push, or recreate a published release tag. If product or workflow source must change after the tags were created, fix the problem and release a new patch version.

After the run succeeds, verify both tags and the GitHub Release:

```bash
VERSION=0.226.2
git ls-remote --tags origin "refs/tags/$VERSION" "refs/tags/$VERSION^{}" "refs/tags/v$VERSION" "refs/tags/v$VERSION^{}"
gh release view "$VERSION"
```
