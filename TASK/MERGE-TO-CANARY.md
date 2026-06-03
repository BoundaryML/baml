# Merge To Canary Notes

This file records how to merge `paulo/baml-language-release-pipeline` with the newer `canary` branch. It is intentionally written as merge guidance, not as a new product spec. `TASK/TASK.md` remains the source of truth for the target release architecture.

## Pre-Merge Branch Snapshot

This snapshot captured the state before the canary merge was performed. Do not treat these SHAs as current branch metadata; keep them only as historical context for why the merge had the conflict shape described below.

- branch: `paulo/baml-language-release-pipeline`
- merge base: `1742580cac5049c3e6c6eca8ba48f601a0b9bda1`
- branch head: `12463db4a787f1ec199039d8ccc6726ce19e7929`
- `origin/canary`: `9cff3559908d46a40041fce648764bfb38ed5052`
- divergence: branch is 3 commits ahead and 17 commits behind `origin/canary`

True merge conflicts from `git merge-tree --write-tree HEAD origin/canary`:

1. `.github/workflows/release-baml-language-alpha.yml`
2. `baml_language/sdks/nodejs/bridge_nodejs/package.json`

Several other files changed on both sides and auto-merge, but still need a semantic pass because they touch release plumbing or SDK/version surfaces.

## Merge Invariants

Use these as the decision filter while resolving conflicts:

- `TASK/TASK.md` remains the product spec; this file is only a merge playbook.
- There is one BAML language/toolchain release plan. Python, VSIX/TypeScript, Node, Rust public version APIs, and future SDK surfaces read the frozen plan or files stamped from it.
- The wrapper is a separate product. Homebrew/AUR package the wrapper only and track `baml_language/crates/baml/Cargo.toml` `[package].version`, not the language/toolchain version.
- Old release workflows are deleted or folded into the new graph. They must not survive as independent fallback publishers with their own version authority.
- Canary's product work is preserved when it does not violate those boundaries, especially the new Node SDK package shape and test/build work.

## Conflict 1: Alpha Release Workflow

File:

```text
.github/workflows/release-baml-language-alpha.yml
```

Conflict type:

```text
modify/delete
```

Our branch deletes this workflow. `canary` modifies it to keep the old alpha release path alive while updating some infrastructure, including Blacksmith checkout usage.

Resolution:

```text
keep deleted
```

Reason:

- `TASK/TASK.md` explicitly lists `.github/workflows/release-baml-language-alpha.yml` as dead code to delete.
- The new release graph entrypoint is `.github/workflows/release-baml-language.yml`.
- Keeping the alpha workflow would preserve the old `0.11.0-alpha.<n>` release path that this task is replacing with canary/nightly release planning.
- The alpha workflow still calls old alpha semantics such as `scripts/baml-language-version compute --alpha-number`, old Homebrew tap dispatch paths, and old package-manager coupling.

What to learn from `canary` before deleting:

- `canary` has moved parts of CI to Blacksmith infrastructure, for example `useblacksmith/checkout@v1` and Blacksmith runners.
- Do not retain the alpha workflow to get those changes.
- Instead, ensure the replacement release graph uses the current CI infrastructure patterns where appropriate.

Follow-up check for the new release graph:

- Audit `.github/workflows/release-baml-language.yml` after merge.
- Prefer `useblacksmith/checkout@v1` and Blacksmith runners for jobs where this repo has standardized on them.
- Keep `persist-credentials: false` on checkout steps unless the checked-out repo itself needs the default GitHub token.
- Publishing jobs may still need GitHub-hosted runners or specific permissions, especially for OIDC publishing.

In short: delete the alpha workflow, but do not ignore the infrastructure modernization that landed on `canary`.

Cutover gate:

- Keeping this file deleted is the correct branch resolution.
- Merging the branch to `canary` is only safe after the replacement release graph has demonstrated equivalent `baml-pack-host` coverage:
  - per-target `baml-toolchain` archives contain `bin/baml-cli`, `bin/baml-pack-host`, and `assets/baml-vscode.vsix`;
  - manifests include the per-target archive URLs and checksums;
  - `baml pack --target <non-host-target>` succeeds against a dry-run or production new-graph manifest;
  - that pack test uses the shared `baml_release` fetcher path, not alpha-release-only assets.

Why this gate exists:

- The alpha workflow is the old production path for language release artifacts.
- Deleting it before the replacement path can publish and consume `baml-pack-host` would leave cross-target `baml pack` without a proven release source.
- Therefore: delete in this branch, validate the replacement path before merge/cutover.

## Conflict 2: Node SDK Package Manifest

File:

```text
baml_language/sdks/nodejs/bridge_nodejs/package.json
```

Conflict type:

```text
content
```

Our side changes only:

```json
"version": "0.11.0"
```

`canary` substantially rewrites the package:

- package name changes from `@boundaryml/baml-node` to `@boundaryml/baml-core-node`;
- package becomes ESM with `"type": "module"`;
- package exports through `./dist/index.js`;
- package publishes `dist`;
- build scripts target `dist/native.js`;
- test runner changes from Jest to Vitest;
- `@napi-rs/cli`, `protobufjs`, and related tooling are updated;
- npm publish support is added through `release-sdk.yaml` and `build-nodejs-sdk.reusable.yaml`.

Resolution:

```text
keep canary's new Node SDK package shape, but preserve our canonical version-stamping intent
```

Practical resolution:

- Start from `origin/canary:baml_language/sdks/nodejs/bridge_nodejs/package.json`.
- Keep the new `@boundaryml/baml-core-node` / ESM / `dist` / Vitest shape.
- Set `"version"` to the canonical BAML language/toolchain version controlled by our release stamping, currently `0.11.0` in the branch.
- Ensure `scripts/baml-language-version stamp` continues to update this file through `NODE_PACKAGE`.

Why:

- The Node SDK shape on `canary` is real product work and should not be reverted.
- The release architecture task says generated/runtime SDK surfaces should share the selected BAML language/toolchain version.
- The only part of our side that matters for this file is version authority, not the old package layout.

Required post-resolution checks:

- `scripts/baml-language-version stamp --plan <release-plan.json>` updates `baml_language/sdks/nodejs/bridge_nodejs/package.json`.
- `baml_language/sdks/nodejs/bridge_nodejs/src/lib.rs` keeps `get_version()` returning `baml_version::CANONICAL_VERSION`.
- `baml_language/Cargo.toml` keeps both canary's Node SDK/test additions and our `baml_version` / `baml_release` workspace dependencies.
- `baml_language/stow.toml` keeps both canary namespace additions and our wrapper/release exceptions.

## Should Node SDK Be Modeled Now?

Short answer:

```text
Model Node versioning now. Defer full Node npm publishing by default.
```

What should happen now:

- Keep `@boundaryml/baml-core-node` as a first-class version-stamped SDK surface.
- Keep `baml_language/sdks/nodejs/bridge_nodejs/package.json` in `scripts/baml-language-version` stamping.
- Keep `bridge_nodejs::get_version()` wired to `baml_version::CANONICAL_VERSION`.
- Preserve canary's Node SDK package shape and test/build changes.
- Add enough release-plan language/comments so future SDK publish jobs can consume the same frozen `release-plan.json` rather than reading independent package versions.
- If `build-nodejs-sdk.reusable.yaml` is kept, adapt it to accept `release_plan_json` and run `scripts/baml-language-version stamp --plan release-plan.json` before any build that reads `package.json` or Rust version constants.

What should not happen casually in this merge:

- Do not revive standalone `release-sdk.yaml` as an independent source of release truth.
- Do not let Node package version be manually bumped outside the BAML language release plan.
- Do not publish npm from an old independent SDK release path that can drift from PyPI/toolchain versions.
- Do not wire production npm publishing into `.github/workflows/release-baml-language.yml` unless this PR is explicitly re-scoped to own npm release behavior and validation.

Recommended staged path:

1. **Now, in this merge:** preserve Node SDK code/package changes and canonical version stamping.
2. **Now, optional but non-publishing:** preserve or add a Node build/test reusable workflow only if it consumes `release_plan_json` and remains a rehearsal/CI validation path.
3. **Later:** add a focused `publish-nodejs-npm.yml` or equivalent `publish-npm` job called by `release-baml-language.yml`, using the same release-plan contract.
4. **Later:** expand the release graph to other SDKs using the same release-plan contract.

The important architectural boundary is not "Python only." It is "one release plan owns all BAML language/toolchain version surfaces." Node can join that model now at the version-surface layer, even if npm publishing is deferred.

## Semantic Overlap: `release-sdk.yaml`

File:

```text
.github/workflows/release-sdk.yaml
```

Git may not surface this as a direct conflict because our branch renames/reframes the Python publisher as:

```text
.github/workflows/publish-python-pypi.yml
```

But this is a semantic conflict.

`canary` updates `release-sdk.yaml` to:

- build Python wheels;
- build Node native addons through `build-nodejs-sdk.reusable.yaml`;
- publish Python to PyPI;
- publish `@boundaryml/baml-core-node` to npm.

Our task says:

- rename/integrate the Python release path into the BAML language release graph;
- do not keep independently publishing SDKs with separate version logic;
- future SDK publishers should scale as focused workflows, but fan out/fan in through one release plan.

Recommended resolution:

- Keep our `publish-python-pypi.yml` as the Python publisher called by `release-baml-language.yml`.
- Do not keep `release-sdk.yaml` as an independent release orchestrator in its canary form.
- Preserve the useful Node build work by adapting it into the new graph or CI:
  - keep `build-nodejs-sdk.reusable.yaml` if it is usable as a build/test matrix;
  - add a `release_plan_json` input before relying on it for release builds;
  - stamp before `pnpm install`, `napi build`, TypeScript build, tests, or package assembly;
  - ensure npm package version comes from stamped `package.json`, not an independent manual bump.
- Preserve the useful npm publish implementation as reference material, but do not carry it forward as active production publishing unless this PR is explicitly expanded beyond `TASK/TASK.md` Phase 4's deferred npm path.

If we defer npm publishing, document that `release-sdk.yaml`'s Node publish logic is intentionally not carried forward yet, rather than accidentally deleted.

## Auto-Merged But Relevant Files

These files changed on both sides and should be reviewed after the merge even if Git auto-merges them.

### `.github/workflows/ci.yaml`

Why relevant:

- The new release graph uses `workflow_run` on workflow name `CI - BAML Language`.
- `canary` changes CI structure, Blacksmith checkout, SDK test matrices, and perf/size-gate behavior.

Expected resolution:

- Keep the workflow name `CI - BAML Language`.
- Keep canary's CI modernization.
- Preserve any release-graph triggers or change-detection assumptions introduced by this branch.
- If the new release graph depends on canary branch pushes being green before publishing nightly, confirm the `workflow_run` event still fires on pushes to `canary`.
- Update CI change detection so new release-architecture files are not invisible:
  - `.github/workflows/release-baml-language.yml`
  - `.github/workflows/publish-python-pypi.yml`
  - `scripts/baml-release-manifests`
  - `scripts/baml-package-manager-artifacts`
  - `scripts/baml-wrapper-version`
  - `scripts/install.sh`
  - `scripts/install.ps1`

### `baml_language/Cargo.toml`

Why relevant:

- Our branch adds `baml_release` and `baml_version`.
- `canary` adds forked AWS/GCP crates, Node SDK/test crates, and dependency version updates.

Expected resolution:

- Keep both sets of changes.
- Ensure workspace members include new canary SDK/test crates and our wrapper/release/version crates.
- Ensure `[workspace.dependencies]` includes `baml_release`, `baml_version`, and canary's new forked dependency entries.

### `baml_language/Cargo.lock`

Why relevant:

- Both sides change dependency graph substantially.

Expected resolution:

- Regenerate/accept the lockfile after resolving `Cargo.toml`.
- Validate with:

```bash
cd baml_language
mise run fmt
mise run clippy
```

### `baml_language/crates/baml_cli/Cargo.toml`

Why relevant:

- Our branch adds `baml_release` and `baml_version`.
- `canary` adds `codegen_nodejs`.

Expected resolution:

- Keep all three dependencies if used by the merged code.

### `baml_language/sdks/nodejs/bridge_nodejs/src/lib.rs`

Why relevant:

- Our branch changes `get_version()` to `baml_version::CANONICAL_VERSION`.
- `canary` exports new Node modules such as `media` and `runtime`.

Expected resolution:

- Keep canary's module exports.
- Keep `get_version()` returning `baml_version::CANONICAL_VERSION`.

### `baml_language/sdks/python/uv.lock`

Why relevant:

- Our branch stamps `baml_core` version.
- `canary` changes Python dependency metadata.

Expected resolution:

- Regenerate or accept a consistent lockfile after stamping.
- Confirm `baml_language/sdks/python/pyproject.toml` and `baml_core.__version__` are still controlled by `scripts/baml-language-version`.

### `baml_language/stow.toml`

Why relevant:

- Our branch adds wrapper/release exceptions.
- `canary` adds forked AWS/GCP namespace exceptions.

Expected resolution:

- Keep both.
- Run `cd baml_language && mise run stow`.

## Blacksmith And Caching Checklist

When resolving the merge, inspect the replacement release workflows against current canary CI conventions.

Checklist:

- Use Blacksmith runners where the repo now expects them for BAML language CI/release jobs.
- Prefer `useblacksmith/checkout@v1` where canary has standardized on it.
- Keep `persist-credentials: false`.
- Preserve existing setup/cache actions for Rust, Node, Python, and pnpm where canary improved them.
- Do not copy Blacksmith changes by retaining deprecated alpha release jobs.
- For release jobs requiring OIDC or package-registry permissions, verify runner constraints before switching.

Concrete workflow audit:

- `.github/workflows/release-baml-language.yml` currently uses several `actions/checkout@v4` steps. Replace with `useblacksmith/checkout@v1` where compatible with the job and runner.
- Build jobs should prefer the repo's setup actions, such as `./.github/actions/setup-rust` and `./.github/actions/setup-node`, when those actions provide the Blacksmith/cache behavior canary now relies on.
- Publishing jobs may need GitHub-hosted runners for OIDC or registry support; keep those exceptions explicit in comments.
- Do not move secrets-bearing package-manager publish jobs to a different runner class without validating token/OIDC behavior.

## Validation After Conflict Resolution

Minimum validation:

```bash
git status --short
cd baml_language
mise run fmt
mise run stow
mise run clippy
```

Focused release/script validation:

```bash
scripts/baml-wrapper-version show
scripts/baml-language-version compute --channel canary
scripts/baml-language-version compute --channel nightly
python3 -m py_compile \
  scripts/baml-language-version \
  scripts/baml-package-manager-artifacts \
  scripts/baml-release-manifests \
  scripts/baml-wrapper-version
sh -n scripts/install.sh
```

Focused frontend/VSIX validation:

```bash
corepack pnpm --dir typescript2 --filter baml-language typecheck
```

Node SDK validation after adopting canary's Node package shape:

```bash
cd baml_language/sdks/nodejs/bridge_nodejs
pnpm install --ignore-scripts
pnpm build:proto
pnpm build:ts_build
pnpm test
```

Release cutover validation:

```bash
gh workflow run release-baml-language.yml \
  --ref paulo/baml-language-release-pipeline \
  -f channel=nightly \
  -f dry_run=true
```

After the dry-run produces manifests/artifacts, validate a clean install path with a temporary `BAML_HOME` and dry-run manifest base URL, then validate cross-target pack:

```bash
BAML_HOME="$(mktemp -d)" \
BAML_MANIFEST_BASE_URL="<dry-run-manifest-base-url>" \
  path/to/baml toolchain use nightly

BAML_HOME="<same-temp-home>" \
BAML_MANIFEST_BASE_URL="<dry-run-manifest-base-url>" \
  path/to/baml pack --target <non-host-target>
```

If Node npm publishing is intentionally wired into the new release graph now, also dry-run the Node build matrix and npm package assembly before merge. By default, npm publishing remains deferred.

## Decision Summary

- Delete the alpha workflow conflict.
- Keep canary's Node SDK package architecture.
- Preserve canonical BAML language version stamping for Node.
- Treat `release-sdk.yaml` as a semantic conflict, not harmless drift.
- Keep canary's Blacksmith/caching improvements in the replacement workflows where compatible.
- Model Node as a version-stamped SDK surface now.
- Defer full npm publishing by default; do not keep an independent SDK release path as the fallback.
- Keep the alpha workflow deleted in this branch, but do not merge/cut over until the replacement graph proves `baml-pack-host` and cross-target `baml pack` work through the new manifest/fetcher path.
