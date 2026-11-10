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
4. Users with `baml toolchain use nightly -g` can now pick up the new release with `baml toolchain update`. This advances the active selector, so run it outside a project with a toolchain pin and with `BAML_TOOLCHAIN` unset to update the user default.

## Canary release

1. Merge a PR into `canary` to bump the release version in 20+ files in the repo: [release.toml](release.toml) and every BAML bridge's release version metadata, e.g. `pyproject.toml` or `package.json`. [Example](https://github.com/BoundaryML/baml/pull/4629).
2. Once post-merge CI passes, automation creates the `baml-language-source-<sha>` tag and starts [release-baml-language.yml](../.github/workflows/release-baml-language.yml) with the `canary` channel and the new version.
3. `release-baml-language.yml` runs, first building everything, then publishing the individual pieces, and updating the canary channel manifest once its prerequisites succeed.
4. Users with `baml toolchain use canary -g` can now pick up the new release with `baml toolchain update`. This advances the active selector, so run it outside a project with a toolchain pin and with `BAML_TOOLCHAIN` unset to update the user default.

## What gets published

Canary and nightly releases ship the language toolchain and SDKs under the selected release version, with registry-specific version formatting where needed.

- Toolchain: `baml toolchain use 0.18.0 -g`
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

The wrapper is the `baml` command that installs, selects, and launches language toolchains. It is versioned independently. Use `scripts/baml-wrapper-version bump --patch` to prepare a wrapper patch. To trigger an immediate canary release, also run `scripts/baml-language-version bump --patch`; this releases the language and SDKs along with the wrapper. There is no wrapper-only dispatcher.

1. Prepare a PR that bumps the wrapper version in its [package manifest](crates/baml/Cargo.toml) when wrapper changes need to ship. For a first wrapper release, talk to Paulo or Avery before changing that version, as the manifest requests.
2. Merge the PR into `canary`. The next language release containing the bump, whether canary or nightly, also publishes the wrapper if its version differs from the currently published wrapper. A wrapper-only bump does not by itself trigger an immediate canary release.
3. The release workflow publishes wrapper archives and checksums (x64-linux-gnu, aarch64-linux-gnu, x64-linux-musl, aarch64-linux-musl, x64-darwin, aarch64-darwin, x64-windows, aarch64-windows) and a `baml-wrapper-<version>` GitHub release and tag, updates the wrapper download catalog, and tests and updates the BoundaryML Homebrew tap.
4. Follow the release workflow and any resulting Homebrew Core version-bump PR through to success. Homebrew Core may open that PR automatically; its completion needs separate attention from the language release workflow.

The person arranging the wrapper release must plan to monitor both the workflow and Homebrew follow-through, with the current oncall retaining responsibility for release failures. Users receive wrapper updates through the standalone installer's self-update support or their package manager. A language release with no wrapper version change reuses the published wrapper.

The published wrapper is the packed BAML implementation in `wrapper/`. `build2-baml-wrapper.reusable.yaml` uses the compiler and per-target pack hosts from this release’s toolchain artifacts, then `scripts/baml-pack-wrapper-release` produces the canonical `baml-wrapper[-no-self-update]-<version>-<target>` archives. `wrapper-baml` contains only those archives in a flat directory for the release, manifest, Homebrew, AUR binary, and WinGet publishers, which download this artifact by name. Rust reference archives (`rust-wrapper-<target>` Actions artifacts) remain in the shared release tests but cannot enter publishing downloads. The AUR source package builds its own compiler and pack host, then packs the BAML wrapper with self-update disabled; source builds omit embedded BCS credentials. The generated PKGBUILD sets `BAML_GIT_SHA` to the release commit so compiler and host fingerprints agree even without Git metadata in the source archive.

Before running a release, configure the `BAML_WRAPPER_BOUNDARY_API_KEY`, `BAML_WRAPPER_BCS_PROJECT`, and `BAML_WRAPPER_BCS_ENVIRONMENT` secrets in the `infisical-github-baml-language-release` GitHub environment. Use an ordinary API key owned by the configured organization/project with exclusive write access to that one environment. The wrapper manifest selects the secret with `[pack.build] embed_telemetry_api_key_from_env_var = "BAML_WRAPPER_BOUNDARY_API_KEY"`; the builder writes the expected project and environment into `boundary.project` and `pack.build.embed_telemetry_env`. Packing sends `{"fullResponse":true}` to `POST /v1/auth/verify`, requires its complete `200 OK` JSON response, checks ownership and effective permissions, and embeds the exact key unchanged. This is the only embedding HTTP operation; it sends no source, bytecode, or digest. Every platform and both self-update variants use the same contract. Runtime bindings allow caller overrides through ordinary API-key destination selection. Deploy BCS introspection support before shipping these clients; older servers returning 204 cannot satisfy embedding checks. Existing `baml cloud auth status` clients keep the default 204 contract. Older artifacts with minted build tokens must be rebuilt with ordinary API keys.

Missing, empty, or invalid embedding credentials, unsupported verification responses, verification-service failures, and ownership or permission mismatches block packing and publishing. This cutover keeps the wrapper version source and publication rules above; publishing a new wrapper still requires a wrapper version that differs from the current download catalog.

Dry runs also install the generated Homebrew formula from the exact local wrapper archives, run its test block, and verify linkage before reporting success. The rehearsal uses a separate keg-only formula and removes its temporary tap and installation afterward. The formula's tests select an executable local CLI fixture because `baml --version` delegates to the active CLI and installs missing managed toolchains; package tests should not bootstrap a language release.

Dry runs also build and install the generated binary AUR package in an Arch Linux container, compare both packages' `.SRCINFO` against `makepkg`, verify the installed executable matches the release archive byte for byte, and smoke-test the wrapper. Both AUR recipes disable stripping because rewriting a packed executable destroys its BAML payload. The source recipe also disables Arch's added LTO and debug flags: Cargo controls those settings, and GCC LTO objects in bundled C libraries cannot be linked by Rust's LLVM linker.
