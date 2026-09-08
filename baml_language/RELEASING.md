# BAML v1 Releases

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
- [What gets published](#what-gets-published): released artifacts, package destinations, and architectures.
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

- GitHub release: `baml-language-<version>-<target>` toolchain archives containing `baml-cli`, `baml-pack-host`, the VS Code extension, and playground assets (x64-linux-gnu, aarch64-linux-gnu, x64-linux-musl, aarch64-linux-musl, x64-darwin, aarch64-darwin, x64-windows, aarch64-windows).
- GitHub release: `baml-language-<version>.vsix` VS Code extension (platform independent).
- GitHub release: `libbaml_cffi` shared libraries, named `baml_cffi` on Windows (x64-linux-gnu, aarch64-linux-gnu, x64-linux-musl, aarch64-linux-musl, x64-darwin, aarch64-darwin, x64-windows, aarch64-windows).
- PyPI: `baml-bridge` wheels (x64-linux-gnu, aarch64-linux-gnu, x64-linux-musl, aarch64-linux-musl, x64-darwin, aarch64-darwin, x64-windows, aarch64-windows).
- npm: `@boundaryml/baml-bridge` and its platform packages (x64-linux-gnu, aarch64-linux-gnu, x64-linux-musl, aarch64-linux-musl, x64-darwin, aarch64-darwin, x64-windows, aarch64-windows). Canary publishes under `latest`; nightly publishes under `nightly`.
- npm: `@boundaryml/baml-bridge-web` (WebAssembly, `wasm32-unknown-unknown`), using the same npm tags.
- Maven Central: `com.boundaryml:baml-bridge`, including main, sources, Javadoc, and native JARs (x64-linux-gnu, aarch64-linux-gnu, x64-darwin, aarch64-darwin, x64-windows; experimental builds for x64-linux-musl, aarch64-linux-musl, and aarch64-windows may be omitted if they fail).
- Maven Central: `com.boundaryml:baml-bridge-kotlin`, `com.boundaryml:baml-gradle-plugin`, and the `com.boundaryml.baml` plugin marker (JVM). Canary also publishes the plugin to the Gradle Plugin Portal.
- NuGet: `baml-bridge`, including native runtimes (x64-linux-gnu, aarch64-linux-gnu, x64-linux-musl, aarch64-linux-musl, x64-darwin, aarch64-darwin, x64-windows, aarch64-windows).
- crates.io: `baml_bridge` (source crate; uses the shared native libraries listed above).
- Go: [`github.com/boundaryml/baml-go`](https://github.com/BoundaryML/baml-go) version tag (source module; uses the shared native libraries listed above).
- Swift: [`BoundaryML/baml-swift`](https://github.com/BoundaryML/baml-swift) version tag and `BamlBridgeFFI-<version>.xcframework.zip` on the GitHub release (x64-darwin, aarch64-darwin, aarch64-ios, x64-ios-simulator, aarch64-ios-simulator).
- GitHub release: SHA-256 checksum files for toolchain archives, the VS Code extension, and shared native libraries.
- `pkg.boundaryml.com`: version and channel manifests, the download page, and `install.sh` / `install.ps1` installers.
- `developer.boundaryml.com`: the generated developer reference for the released toolchain.

Publication happens across several destinations. Packages and the GitHub release can become visible before the whole release succeeds. The toolchain channel advances only after its required publishing, public-package checks, and documentation steps complete; further artifact verification runs afterward.

# BAML wrapper releases

The wrapper is the `baml` command that installs, selects, and launches language toolchains. It has an independent version and is shared by the canary and nightly channels. Updating the wrapper does not itself change the user's selected language toolchain.

1. Prepare a PR that bumps the wrapper version in its [package manifest](crates/baml/Cargo.toml) when wrapper changes need to ship. For a first wrapper release, talk to Paulo or Avery before changing that version, as the manifest requests.
2. Merge the PR into `canary`. The next language release containing the bump, whether canary or nightly, also publishes the wrapper if its version differs from the currently published wrapper. A wrapper-only bump does not by itself trigger an immediate canary release.
3. The release workflow publishes wrapper archives and checksums (x64-linux-gnu, aarch64-linux-gnu, x64-linux-musl, aarch64-linux-musl, x64-darwin, aarch64-darwin, x64-windows, aarch64-windows) and a `baml-wrapper-<version>` GitHub release and tag, updates the wrapper download catalog, and tests and updates the BoundaryML Homebrew tap.
4. Follow the release workflow and any resulting Homebrew Core version-bump PR through to success. Homebrew Core may open that PR automatically; its completion needs separate attention from the language release workflow.

The person arranging the wrapper release must plan to monitor both the workflow and Homebrew follow-through, with the current oncall retaining responsibility for release failures. Users receive wrapper updates through the standalone installer's self-update support or their package manager. A language release with no wrapper version change reuses the published wrapper.
