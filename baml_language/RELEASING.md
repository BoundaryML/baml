# BAML v1 Releases

The BAML language release process runs as follows:

1. A human or agent merges a PR into `canary` that bumps the language release version and updates the corresponding package versions.
2. Once CI passes for that commit, GitHub Actions starts a canary release from that exact source. Nightly releases use a separate daily schedule to select a green commit.
3. The release workflow builds the toolchain and SDKs, then publishes packages and downloadable artifacts. During publication, it creates the GitHub release and attaches the version tag to the source commit.
4. Once publishing and the required checks of published packages succeed, the workflow publishes the matching developer documentation and advances the channel to the new release.
5. The workflow runs further artifact verification and reports the result in Slack. The current oncall follows the release through to completion and fixes any failures.

**The [current oncall](../tools/bctl_src/oncall/data/schedule.oncall) is responsible for getting a canary release out every Friday and for keeping the release workflows working.**

## Table of contents

- [Nightly and canary](#nightly-and-canary): release cadence, source selection, and version names.
- [Preparing a canary release](#preparing-a-canary-release): what belongs in the release PR and what starts publication.
- [What gets published](#what-gets-published): the toolchain, SDKs, editor package, and documentation.
- [BAML wrapper releases](#baml-wrapper-releases): the wrapper's separate version, publication, and package manager follow-through.

## Nightly and canary

Both channels release code from the `canary` branch. The branch name and the release channel are separate concepts: merging ordinary changes makes them eligible for a nightly; a language version bump requests a canary release.

| | Canary | Nightly |
| --- | --- | --- |
| Purpose | The deliberately selected release we ship each Friday. | A daily way to try newer changes between canary releases. |
| Trigger | A merged language version bump followed by green CI on that commit. | The [nightly schedule](https://github.com/BoundaryML/baml/actions/workflows/nightly-release.yml), around midnight Pacific time. |
| Source | The exact version-bump commit. | The newest eligible `canary` commit with successful CI. |
| Version | The version in [release.toml](release.toml), such as `0.18.0`. | The next patch version with a nightly suffix, such as `0.18.1-nightly.20260907.a`. |

The nightly date names the Pacific day that just ended; the letter distinguishes additional cuts for the same date. Registries may represent this version differently to fit their own version rules.

A night with no new eligible changes normally produces no release. The schedule also avoids starting another release when a successful release already exists or a run is in progress for the selected commit, so a canary cut can cause that night's nightly to be skipped. Nightlies do not require a version-bump PR, and releasing a canary does not immediately trigger a nightly.

Friday is our release commitment, rather than an automatic canary schedule. Oncall must arrange the version bump and follow the resulting release. Additional canary releases can use the same process when needed.

## Preparing a canary release

1. Choose a new, unreleased language version and prepare a PR with the intended release changes.
2. Update [release.toml](release.toml) and the corresponding package versions together using the [language version tooling](../scripts/baml-language-version). Review the resulting changes and describe what is shipping in the PR.
3. Merge into `canary`, then follow [CI - BAML Language](https://github.com/BoundaryML/baml/actions/workflows/ci.yaml) for the merged commit. Successful CI automatically starts [BAML Language Release](https://github.com/BoundaryML/baml/actions/workflows/release-baml-language.yml) for the new version.

The release uses that tested commit even if more PRs merge while it is running. GitHub Actions creates the language release tag, `baml-language-<version>`, during publication; creating a tag manually is not part of starting a language release.

The wrapper has its own version and release responsibilities, described [below](#baml-wrapper-releases). A normal language release only needs a language version bump.

## What gets published

Canary and nightly releases ship the language toolchain and SDKs under the selected release version, with registry-specific version formatting where needed.

| Deliverable | Destination |
| --- | --- |
| Language toolchain, VS Code extension package, and native libraries | [GitHub releases](https://github.com/BoundaryML/baml/releases) |
| Python SDK | [baml-bridge on PyPI](https://pypi.org/project/baml-bridge/) |
| Node.js and browser SDKs | [@boundaryml/baml-bridge](https://www.npmjs.com/package/@boundaryml/baml-bridge) and [@boundaryml/baml-bridge-web](https://www.npmjs.com/package/@boundaryml/baml-bridge-web) on npm; canary uses `latest`, nightly uses `nightly` |
| Java and Kotlin SDKs | [baml-bridge](https://central.sonatype.com/artifact/com.boundaryml/baml-bridge) and [baml-bridge-kotlin](https://central.sonatype.com/artifact/com.boundaryml/baml-bridge-kotlin) on Maven Central |
| Gradle plugin | [Maven Central](https://central.sonatype.com/artifact/com.boundaryml/baml-gradle-plugin) for both channels; also the [Gradle Plugin Portal](https://plugins.gradle.org/plugin/com.boundaryml.baml) for canary |
| C# SDK | [baml-bridge on NuGet](https://www.nuget.org/packages/baml-bridge) |
| Rust SDK | [baml_bridge on crates.io](https://crates.io/crates/baml_bridge) |
| Go and Swift SDKs | [baml-go](https://github.com/BoundaryML/baml-go) and [baml-swift](https://github.com/BoundaryML/baml-swift) package mirrors, with supporting release assets |
| Toolchain download catalog and channel selection | [pkg.boundaryml.com](https://pkg.boundaryml.com) |
| Generated developer reference | [developer.boundaryml.com](https://developer.boundaryml.com), matched to the released toolchain |

Publication happens across several destinations. Packages and the GitHub release can become visible before the whole release succeeds. The toolchain channel advances only after its required publishing, public-package checks, and documentation steps complete; further artifact verification runs afterward.

# BAML wrapper releases

The wrapper is the `baml` command that installs, selects, and launches language toolchains. It has an independent version and is shared by the canary and nightly channels. Updating the wrapper does not itself change the user's selected language toolchain.

1. Prepare a PR that bumps the wrapper version in its [package manifest](crates/baml/Cargo.toml) when wrapper changes need to ship. For a first wrapper release, talk to Paulo or Avery before changing that version, as the manifest requests.
2. Merge the PR into `canary`. The next language release containing the bump, whether canary or nightly, also publishes the wrapper if its version differs from the currently published wrapper. A wrapper-only bump does not by itself trigger an immediate canary release.
3. The release workflow publishes wrapper downloads and a `baml-wrapper-<version>` GitHub release and tag, updates the wrapper download catalog, and tests and updates the [BoundaryML Homebrew tap](https://github.com/BoundaryML/homebrew-tap).
4. Follow the release workflow and any resulting Homebrew Core version-bump PR through to success. Homebrew Core may open that PR automatically; its completion needs separate attention from the language release workflow.

The person arranging the wrapper release must plan to monitor both the workflow and Homebrew follow-through, with the current oncall retaining responsibility for release failures. Users receive wrapper updates through the standalone installer's self-update support or their package manager. A language release with no wrapper version change reuses the published wrapper.
