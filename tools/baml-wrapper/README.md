This is the standalone BAML implementation of the `baml` toolchain wrapper. It resolves environment, nearest project, user default and fallback selectors; installs absent managed toolchains; manages shared channels; and launches the selected CLI with inherited stdin/stdout/stderr and its exit status. Selection and installation decisions are BAML code. Generic native stdlib operations provide process supervision, filesystem operations, TOML edits and validated archive extraction.

Build with `python3 tools/baml-wrapper/build --cli /absolute/path/to/baml-cli --output /absolute/path/to/baml`. The compiler and adjacent pack host must come from this checkout. `--no-self-update` produces a package-manager variant. The wrapper version comes from the Rust wrapper's Cargo manifest. Builds prepare a temporary project with those constants and invoke `baml pack`; no source files are rewritten. For cross-target packaging, pass `--target <triple> --pack-host /path/to/the/matching/baml-pack-host` to embed the host built from the same commit without fetching a released runtime.

Release builds pass `--embed-telemetry --telemetry-environment <environment>` to this build helper. Set `BOUNDARY_PROJECT=org/project` and authenticate the builder with a mint-capable `BOUNDARY_API_KEY` or saved administrator login. Packing provisions and embeds a public BCS ingestion credential; the builder’s private credential is not included. Minting failure fails the build. Local and conformance builds omit `--embed-telemetry` and remain usable without BCS credentials. The packed wrapper accepts `BAML_WRAPPER_BCS_TELEMETRY=off` to disable BCS recording, and excludes HTTP bodies from network spans. This recording is separate from the wrapper-owned PostHog pipeline described below.

The release workflow reads `BAML_WRAPPER_BCS_PROJECT` (`org/project`) and `BAML_WRAPPER_BCS_ENVIRONMENT` (an existing environment) from GitHub variables, and `BOUNDARY_API_KEY` from the `infisical-github-baml-language-release` environment or the caller’s forwarded secret. Configure those values before running wrapper release packaging; missing configuration and rejected minting fail the job before publishing candidate archives. Both self-update variants and every target receive their own embedded credential. Conformance fixtures set `BAML_WRAPPER_BCS_TELEMETRY=off` when exercising uploaded release binaries.

`toolchain use <selector>` updates the closest existing `baml.toml`; `toolchain use --local <selector>` sets the user default in `$BAML_HOME/config.toml`. Output identifies the new selection and the previous selection, including recorded concrete channel versions. The project always wins over the user default, and `BAML_TOOLCHAIN` takes precedence over both. Relative project paths use the manifest directory; environment and user-default paths use the invocation directory. `toolchain pin` is removed. Updates follow the resolved shared channel and preserve selectors. Damaged installations require an explicit `toolchain install <version> --force`.

Root help and wrapper-command help work without a toolchain. Version reporting installs an absent managed CLI, invokes its original version flag, and propagates failures. All other CLI commands, including `update`, `upgrade`, unknown commands, and command help, are forwarded unchanged to the selected CLI. The selected CLI provides independent wrapper/toolchain update guidance for `update` and `upgrade`.

Both wrappers interpret `--project PATH` and `--project=PATH` when selecting the toolchain and forward the original arguments to `baml-cli`. Relative `--project` paths use the shell working directory. Change the working directory with ordinary `cd`; `--directory` is removed from the wrapper and CLI. The wrapper leaves its actual cwd unchanged so the CLI applies forwarded filesystem flags once. Flags after `--` belong to the target program. After the `query` command, `--project` names a cloud project and does not change filesystem toolchain selection; a root-level `--project` before `query` still selects the source project.

`toolchain install` without a selector installs the active environment/project/user/fallback selection. For a recorded channel it installs that recorded concrete version, preserving the mapping; `toolchain update` is the explicit operation that advances a channel. A local-path selection has nothing to install.

`toolchain status [--install] [--json]` reports the active selection and available releases. `--json` writes one versioned resolution object with available releases to stdout. Every non-help toolchain command first attempts to install the active selection; failures are reported on stderr and the requested operation continues. `--install` requires installation to succeed. Recorded channels retain their concrete version; unrecorded channels establish their first mapping. A validated local-path toolchain returns `version: null` and is never executed to discover its version. Detected installation damage remains an error.

```json
{
  "schema_version": 1,
  "selector": "nightly",
  "version": "0.23.1-nightly.20261026.a",
  "source": "/work/project/baml.toml",
  "wrapper_path": "/home/user/.baml/bin/baml",
  "toolchain_path": "/home/user/.baml/toolchains/0.23.1-nightly.20261026.a/bin/baml-cli",
  "managed": true,
  "installed": true,
  "available_releases": { "nightly": "0.23.1-nightly.20261026.a" }
}
```

`source` identifies the manifest, user config, `$BAML_TOOLCHAIN`, or fallback that selected the toolchain. Exact managed versions can be resolved while absent, in which case `installed` is false and `toolchain_path` is the expected installation path. Consumers should accept additional JSON fields in future schema-compatible updates.

Normal channel launches warn when cached metadata identifies a different available release. A passive refresh runs alongside the child CLI, has a five-second network deadline, and is retried at most once per 24 hours, including failures. `[update] auto_check = false` disables refreshes while preserving notices justified by existing cached metadata. Exact versions and local paths do not perform passive checks. `toolchain status` explicitly refreshes metadata after the best-effort installation attempt and never advances a recorded channel.

`baml update` and `baml upgrade` resolve and install the selected CLI, then delegate to it for guidance about independent wrapper and channel updates. Disabled self-update builds print concrete package-manager commands; successful wrapper updates point to the separate toolchain update check. Wrapper-owned errors use the CLI's human-readable `error:` convention, and management commands produce no entry-point return-value output.

Run `python3 tools/wrapper-e2e/run` to compare both implementations and both self-update variants. Release workflows pack candidates for every wrapper platform from the same run's compiler and pack-host artifacts, verify their embedded release fingerprint, and gate the release on conformance. Candidate archives remain separate from the shipping Rust wrapper archives until promotion. The wrapper version is intentionally unchanged; this work does not publish a release.

Both wrappers implement a separate wrapper telemetry pipeline, with `wrapper_invocation`, `wrapper_resolution`, `wrapper_install`, `wrapper_selection`, `wrapper_update`, and `wrapper_self_update` events. They share `$BAML_HOME/wrapper-telemetry.toml` and `$BAML_HOME/wrapper-telemetry/`; `DO_NOT_TRACK`, a disabled wrapper preference, or the disabled global CLI telemetry preference suppress collection and delivery. Events contain coarse wrapper metadata and bounded decisions/outcomes, without raw selectors, paths, arguments, or errors. Delivery occurs only in a detached `__flush-wrapper-telemetry` helper; Rust seals and spawns it before `exec()`. The BAML build derives ingestion constants from the independent Rust wrapper module. See [TELEMETRY.md](../../TELEMETRY.md) for fields and opt-out behavior.
