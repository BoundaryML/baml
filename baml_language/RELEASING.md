# BAML Language Releases

The BAML language release process runs as follows:

1. A human or agent merges a PR into `canary` that bumps the language release version and updates the corresponding package versions.
2. Once CI passes for that commit, GitHub Actions starts a canary release from that exact source. Nightly releases use a separate daily schedule to select a green commit.
3. The release workflow builds the toolchain and SDKs, then publishes packages and downloadable artifacts. During publication, it creates the GitHub release and attaches the version tag to the source commit.
4. Once publishing and the required checks of published packages succeed, the workflow publishes the matching developer documentation and advances the channel to the new release.
5. The workflow runs further artifact verification and reports the result in Slack. The current oncall follows the release through to completion and fixes any failures.

**The current oncall is responsible for getting a canary release out every Friday and for keeping the release workflows working.** Merging the version bump starts the release; oncall owns the outcome.

## Table of contents

- [Nightly release](#nightly-release): scheduled releases, version tags, and updating a nightly toolchain.
- [Canary release](#canary-release): version-bump PRs, release tags, and updating a canary toolchain.
- [What gets published](#what-gets-published): released toolchains, bridges, editor extensions, and documentation.
- [BAML wrapper releases](#baml-wrapper-releases): the wrapper's separate version, publication, and package manager follow-through.

## Nightly release

1. Around midnight Pacific time, the [nightly dispatcher](../.github/workflows/nightly-release.yml) chooses the newest eligible commit on `canary` that has passed CI. Nights with no new eligible changes normally produce no release.
2. The dispatcher uses the commit's existing `baml-language-source-<sha>` tag, created by CI, to start [release-baml-language.yml](../.github/workflows/release-baml-language.yml) with the `nightly` channel and the computed release version. The tag itself does not trigger the release.
3. After the builds succeed, the workflow publishes artifacts and creates the GitHub release and tag `baml-language-<version>`, such as `baml-language-0.18.1-nightly.20260907.a`, at the source commit. The nightly channel advances after the required publishing, checks, and documentation steps succeed.
4. Users who selected nightly with `baml toolchain use nightly` can then pick up the new release with `baml toolchain update`.

Nightly versions use the next patch after the canary version, followed by the Pacific date that just ended and a letter distinguishing additional cuts that day.

## Canary release

1. Merge a PR into `canary` that bumps the BAML language version in [release.toml](release.toml) and the corresponding versions throughout the repo. [Example](https://github.com/BoundaryML/baml/pull/4629).
2. Once CI passes for that commit, automation creates the `baml-language-source-<sha>` tag and starts [release-baml-language.yml](../.github/workflows/release-baml-language.yml) with the `canary` channel and the new version. The channel is explicitly requested, rather than inferred from the tag.
3. After the builds succeed, the workflow publishes artifacts and creates the GitHub release and tag `baml-language-<version>`, such as `baml-language-0.18.0`, at the source commit. The canary channel advances after the required publishing, checks, and documentation steps succeed.
4. Users who selected canary with `baml toolchain use canary` can then pick up the new release with `baml toolchain update`.

## What gets published

Canary and nightly releases ship the language toolchain and SDKs under the selected release version, with registry-specific version formatting where needed.

- Toolchain: `baml toolchain use 0.18.0`
  - CLI itself (per-architecture, packaged in the GitHub release).
  - `pkg.boundaryml.com` release manifests: `/manifest/v1/canary.json`, `/manifest/v1/nightly.json`, and `/manifest/v1/version/<version>.json`.
- VS Code extension: `.vsix` (platform independent, packaged in the GitHub release and toolchain archives).
- BAML bridges
  - CFFI dynamic library: `.dylib`, `.so`, `.dll` (per-architecture, packaged in the GitHub release).
  - Python package: PyPI `baml-bridge`, installed with `uv add baml-bridge` (per-architecture wheels).
  - `typescript/node` npm package: `@boundaryml/baml-bridge` (with per-architecture native packages).
  - `typescript/web` npm package: `@boundaryml/baml-bridge-web` (platform-independent WebAssembly).
    - Includes bridges for browsers (Chrome, Firefox) and Cloudflare Workers.
  - Java package: Maven Central `com.boundaryml:baml-bridge` (with per-architecture native JARs).
  - Kotlin package: Maven Central `com.boundaryml:baml-bridge-kotlin`.
  - Gradle plugin: Maven Central `com.boundaryml:baml-gradle-plugin` and the `com.boundaryml.baml` plugin marker; also published to the Gradle Plugin Portal for canary releases.
  - C# package: NuGet `baml-bridge` (one package containing per-architecture native libraries).
  - Rust crate: crates.io `baml_bridge`.
  - Go release: the bridge runtime is pushed to `github.com/BoundaryML/baml-go` under a new version tag.
  - Swift release: the bridge runtime is pushed to `github.com/BoundaryML/baml-swift` under a new version tag, with `BamlBridgeFFI-<version>.xcframework.zip` published on the GitHub release (macOS, iOS, and iOS Simulator).
- Documentation: the generated developer reference at `developer.boundaryml.com`.

# BAML wrapper releases

The wrapper is the `baml` command that installs, selects, and launches language toolchains. It is versioned independently.

1. Prepare a PR that bumps the wrapper version in its [package manifest](crates/baml/Cargo.toml) when wrapper changes need to ship. For a first wrapper release, talk to Paulo or Avery before changing that version, as the manifest requests.
2. Merge the PR into `canary`. The next language release containing the bump, whether canary or nightly, also publishes the wrapper if its version differs from the currently published wrapper. A wrapper-only bump does not by itself trigger an immediate canary release.
3. The release workflow publishes wrapper archives and checksums (x64-linux-gnu, aarch64-linux-gnu, x64-linux-musl, aarch64-linux-musl, x64-darwin, aarch64-darwin, x64-windows, aarch64-windows) and a `baml-wrapper-<version>` GitHub release and tag, updates the wrapper download catalog, and tests and updates the BoundaryML Homebrew tap.
4. Follow the release workflow and any resulting Homebrew Core version-bump PR through to success. Homebrew Core may open that PR automatically; its completion needs separate attention from the language release workflow.

The person arranging the wrapper release must plan to monitor both the workflow and Homebrew follow-through, with the current oncall retaining responsibility for release failures. Users receive wrapper updates through the standalone installer's self-update support or their package manager. A language release with no wrapper version change reuses the published wrapper.
