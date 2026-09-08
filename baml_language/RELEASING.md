# BAML v1 Releases

The BAML language release process runs as follows:

1. A human or agent merges a PR into `canary` that bumps the language release version and updates the corresponding package versions.
2. Once CI passes for that commit, GitHub Actions starts a canary release from that exact source. Nightly releases use a separate daily schedule to select a green commit.
3. The release workflow builds the toolchain and SDKs, then publishes packages and downloadable artifacts. During publication, it creates the GitHub release and attaches the version tag to the source commit.
4. Once publishing and the required checks of published packages succeed, the workflow publishes the matching developer documentation and advances the channel to the new release.
5. The workflow runs further artifact verification and reports the result in Slack. The current oncall follows the release through to completion and fixes any failures.

**The current oncall is responsible for getting a canary release out every Friday and for keeping the release workflows working.** Merging the version bump starts the release; oncall owns the outcome.

## Table of contents

- [Nightly and canary](#nightly-and-canary): release cadence, source selection, and version names.
- [Friday release ownership](#friday-release-ownership): oncall's responsibilities and handoff expectations.
- [Preparing a canary release](#preparing-a-canary-release): what belongs in the release PR and what starts publication.
- [What gets published](#what-gets-published): the toolchain, SDKs, editor package, and documentation.
- [Confirming completion and handling failures](#confirming-completion-and-handling-failures): how to track a release and recover when it stalls.
- [BAML wrapper releases](#baml-wrapper-releases): the wrapper's separate version, publication, and package manager follow-through.

## Nightly and canary

Both channels release code from the `canary` branch. The branch name and the release channel are separate concepts: merging ordinary changes makes them eligible for a nightly; a language version bump requests a canary release.

| | Canary | Nightly |
| --- | --- | --- |
| Purpose | The deliberately selected release we ship each Friday. | A daily way to try newer changes between canary releases. |
| Trigger | A merged language version bump followed by green CI on that commit. | The nightly schedule, around midnight Pacific time. |
| Source | The exact version-bump commit. | The newest eligible `canary` commit with successful CI. |
| Version | The version in [release.toml](release.toml), such as `0.18.0`. | The next patch version with a nightly suffix, such as `0.18.1-nightly.20260907.a`. |

The nightly date names the Pacific day that just ended; the letter distinguishes additional cuts for the same date. Registries may represent this version differently to fit their own version rules.

A night with no new eligible changes normally produces no release. The schedule also avoids starting another release when a successful release already exists or a run is in progress for the selected commit, so a canary cut can cause that night's nightly to be skipped. Nightlies do not require a version-bump PR, and releasing a canary does not immediately trigger a nightly.

Friday is our release commitment, rather than an automatic canary schedule. Oncall must arrange the version bump and follow the resulting release. Additional canary releases can use the same process when needed.

## Friday release ownership

The current oncall, as recorded in the [oncall schedule](../tools/bctl_src/oncall/data/schedule.oncall), is accountable for the Friday canary release and failures in either release channel.

- Arrange the release PR early enough for review, CI, publication, and repairs to finish on Friday.
- Monitor the source commit's CI and the resulting release workflow until publishing and verification finish.
- Fix broken workflows and release blockers, involving the relevant owners as needed while retaining responsibility for getting the release out.
- If a release is still blocked at handoff, explicitly hand over the affected version, workflow run, blocker, and next action to the incoming oncall.

A green nightly does not replace the Friday canary release. An unresolved release failure remains oncall work even if some artifacts have already published.

## Preparing a canary release

1. Choose a new, unreleased language version and prepare a PR with the intended release changes.
2. Update [release.toml](release.toml) and the corresponding package versions together using the [language version tooling](../scripts/baml-language-version). Review the resulting changes and describe what is shipping in the PR.
3. Merge into `canary`, then follow **CI - BAML Language** for the merged commit. Successful CI automatically starts **BAML Language Release** for the new version.

The release uses that tested commit even if more PRs merge while it is running. GitHub Actions creates the language release tag, `baml-language-<version>`, during publication; creating a tag manually is not part of starting a language release.

The wrapper has its own version and release responsibilities, described [below](#baml-wrapper-releases). A normal language release only needs a language version bump.

## What gets published

Canary and nightly releases ship the language toolchain and SDKs under the selected release version, with registry-specific version formatting where needed.

| Deliverable | Destination |
| --- | --- |
| Language toolchain, VS Code extension package, and native libraries | [GitHub releases](https://github.com/BoundaryML/baml/releases) |
| Python SDK | PyPI |
| Node.js and browser SDKs | npm, under the matching `canary` or `nightly` tag |
| Java SDK | Maven Central |
| Gradle plugin | Gradle Plugin Portal for canary releases; skipped for nightlies |
| C# SDK | NuGet |
| Rust SDK | crates.io |
| Go and Swift SDKs | Their package mirror repositories, with supporting release assets |
| Toolchain download catalog and channel selection | `pkg.boundaryml.com` |
| Generated developer reference | The developer documentation site, matched to the released toolchain |

Publication happens across several destinations. Packages and the GitHub release can become visible before the whole release succeeds. The toolchain channel advances only after its required publishing, public-package checks, and documentation steps complete; further artifact verification runs afterward.

## Confirming completion and handling failures

Track the source commit in [CI - BAML Language](https://github.com/BoundaryML/baml/actions/workflows/ci.yaml) and publication in [BAML Language Release](https://github.com/BoundaryML/baml/actions/workflows/release-baml-language.yml). For a missing nightly, also check [Nightly BAML Language Release](https://github.com/BoundaryML/baml/actions/workflows/nightly-release.yml), which selects the source and starts the release.

Before considering the release done, confirm that the intended version and source match, all required publishing and verification jobs have succeeded, and the corresponding toolchain channel and developer reference have advanced. Slack reports release results in `#general`, but a notification or an existing GitHub tag alone is not sufficient evidence of completion.

For a transient failure, retry the failed jobs in the original release run and monitor the remaining work. A rerun still uses the original source; it does not pick up later fixes merged into `canary`.

If the product or workflow needs a source change, merge the fix and release a new version through the appropriate channel. Preserve published versions and tags. A partially published canary may already be visible to users, and CI may see its existing GitHub release and decline to start another cut of that version. Oncall owns resolving that partial release and verifying the replacement.

# BAML wrapper releases

The wrapper is the `baml` command that installs, selects, and launches language toolchains. It has an independent version and is shared by the canary and nightly channels. Updating the wrapper does not itself change the user's selected language toolchain.

1. Prepare a PR that bumps the wrapper version in its [package manifest](crates/baml/Cargo.toml) when wrapper changes need to ship. For a first wrapper release, talk to Paulo or Avery before changing that version, as the manifest requests.
2. Merge the PR into `canary`. The next language release containing the bump, whether canary or nightly, also publishes the wrapper if its version differs from the currently published wrapper. A wrapper-only bump does not by itself trigger an immediate canary release.
3. The release workflow publishes wrapper downloads and a `baml-wrapper-<version>` GitHub release and tag, updates the wrapper download catalog, and tests and updates the BoundaryML Homebrew tap.
4. Follow the release workflow and any resulting Homebrew Core version-bump PR through to success. Homebrew Core may open that PR automatically; its completion needs separate attention from the language release workflow.

The person arranging the wrapper release must plan to monitor both the workflow and Homebrew follow-through, with the current oncall retaining responsibility for release failures. Users receive wrapper updates through the standalone installer's self-update support or their package manager. A language release with no wrapper version change reuses the published wrapper.
