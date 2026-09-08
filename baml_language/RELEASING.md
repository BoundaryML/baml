# BAML Language Releases

To see the status of the latest ongoing release, visit the status page for [`release-baml-language.yml`](https://github.com/BoundaryML/baml/actions/workflows/release-baml-language.yml). Release failure/success is automatically posted to Slack.

**The current oncall is responsible for getting a canary release out every Friday and for investigating all failed releases.**

Both the nightly and canary release channels follow this process:

1. Something triggers the release. (This is the primary difference between nightly and canary).
2. `release-baml-language.yml` runs:
   1. builds artifacts in parallel where possible
   2. waits for `all-builds` to confirm that all builds succeeded
   3. publishes individual artifacts:
      1. uploads the toolchain to a GitHub release and assigns its tag to the source commit, e.g. `baml-language-0.18.0` or `baml-language-0.18.1-nightly.20260906.a`
      2. publishes bridge packages and the version manifest (e.g. `pkg.boundaryml.com/manifest/v1/version/0.18.0.json`); the version manifest must be available before the Go package publishes
   4. after required publishing, package checks, and documentation succeed, makes the new release available via `baml toolchain update` by updating the `pkg.boundaryml.com` channel manifest (`canary.json` or `nightly.json`)
3. Reports success or failure in Slack `#general`.
4. Users can now pick up the new toolchain with `baml toolchain update`.

## Table of contents

- [Nightly release](#nightly-release): scheduled releases, version tags, and updating a nightly toolchain.
- [Canary release](#canary-release): version-bump PRs, release tags, and updating a canary toolchain.
- [What gets published](#what-gets-published): released toolchains, bridges, editor extensions, and documentation.
- [BAML wrapper releases](#baml-wrapper-releases): the wrapper's separate version, publication, and package manager follow-through.

## Nightly release

Nightly releases are numbered like `0.18.1-nightly.20260906.a`

1. Around midnight Pacific time, the [nightly dispatcher workflow](../.github/workflows/nightly-release.yml) chooses the newest eligible commit on `canary` that has passed CI. Humans/agents can also manually trigger nightly releases.
2. The dispatcher uses the commit's existing `baml-language-source-<sha>` tag, created by CI, to start [release-baml-language.yml](../.github/workflows/release-baml-language.yml) for the `nightly` channel.
3. `release-baml-language.yml` runs, first building everything, then publishing the individual pieces, and updating the nightly channel manifest once its prerequisites succeed.
   1. It computes the nightly release version before building, e.g. `0.18.1-nightly.20260906.a`.
4. Users with `baml toolchain use nightly` can now pick up the new release with `baml toolchain update`.

## Canary release

1. Merge a PR into `canary` to bump the release version in 20+ files in the repo: [release.toml](release.toml) and every BAML bridge's release version metadata, e.g. `pyproject.toml` or `package.json`. [Example](https://github.com/BoundaryML/baml/pull/4629).
2. Once post-merge CI passes, automation creates the `baml-language-source-<sha>` tag and starts [release-baml-language.yml](../.github/workflows/release-baml-language.yml) with the `canary` channel and the new version.
3. `release-baml-language.yml` runs, first building everything, then publishing the individual pieces, and updating the canary channel manifest once its prerequisites succeed.
4. Users with `baml toolchain use canary` can now pick up the new release with `baml toolchain update`.

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
