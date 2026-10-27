This is the standalone BAML implementation of the `baml` toolchain wrapper. It resolves environment, nearest project, user default and fallback selectors; installs absent managed toolchains; manages shared channels; and launches the selected CLI with inherited stdin/stdout/stderr and its exit status. Selection and installation decisions are BAML code. Generic native stdlib operations provide process supervision, filesystem operations, TOML edits and validated archive extraction.

Build with `python3 tools/baml-wrapper/build --cli /absolute/path/to/baml-cli --output /absolute/path/to/baml`. The compiler and adjacent pack host must come from this checkout. `--no-self-update` produces a package-manager variant. The wrapper version comes from the Rust wrapper's Cargo manifest. Builds prepare a temporary project with those constants and invoke `baml pack`; no source files are rewritten. For cross-target packaging, pass `--target <triple> --pack-host /path/to/the/matching/baml-pack-host` to embed the host built from the same commit without fetching a released runtime.

`toolchain use <selector>` updates the closest existing `baml.toml`; `toolchain use --local <selector>` sets the user default in `$BAML_HOME/config.toml`. Output identifies the new selection and the previous selection, including recorded concrete channel versions. The project always wins over the user default, and `BAML_TOOLCHAIN` takes precedence over both. Relative project paths use the manifest directory; environment and user-default paths use the invocation directory. `toolchain pin` is removed. Updates follow the resolved shared channel and preserve selectors. Damaged installations require an explicit `toolchain install <version> --force`.

Both wrappers interpret `--project PATH` and `--project=PATH` when selecting the toolchain and forward the original arguments to `baml-cli`. `--directory PATH` sets the directory used for discovery and relative paths; a relative `--project` is interpreted from that directory. The wrapper leaves its actual cwd unchanged so the CLI applies the forwarded flags once. Flags after `--` belong to the target program. After the `query` command, `--project` names a cloud project and does not change filesystem toolchain selection; a root-level `--project` before `query` still selects the source project.

`toolchain install` without a selector installs the active environment/project/user/fallback selection. For a recorded channel it installs that recorded concrete version, preserving the mapping; `toolchain update` is the explicit operation that advances a channel. A local-path selection has nothing to install.

`toolchain resolve [--install] [--json]` always writes one JSON object to stdout. Without `--install`, it performs no network requests, installation, or CLI invocation. `--install` ensures the selected managed toolchain exists first, with progress on stderr. An unrecorded channel requires `--install`. A validated local-path toolchain returns `version: null` and is never executed to discover its version. Detected installation damage remains an error.

```json
{
  "schema_version": 1,
  "selector": "nightly",
  "version": "0.23.1-nightly.20261026.a",
  "source": "/work/project/baml.toml",
  "wrapper_path": "/home/user/.baml/bin/baml",
  "toolchain_path": "/home/user/.baml/toolchains/0.23.1-nightly.20261026.a/bin/baml-cli",
  "managed": true,
  "installed": true
}
```

`source` identifies the manifest, user config, `$BAML_TOOLCHAIN`, or fallback that selected the toolchain. Exact managed versions can be resolved while absent, in which case `installed` is false and `toolchain_path` is the expected installation path. Consumers should accept additional JSON fields in future schema-compatible updates.

Normal channel launches warn when cached metadata identifies a different available release. A passive refresh runs alongside the child CLI, has a five-second network deadline, and is retried at most once per 24 hours, including failures. `[update] auto_check = false` disables refreshes while preserving notices justified by existing cached metadata. Exact versions and local paths do not perform passive checks. `toolchain status` explicitly refreshes metadata without installing or advancing the selected channel.

`baml update` and `baml upgrade` explain `toolchain update`, `self-update`, and `toolchain status` before resolving or installing a CLI. Disabled self-update builds print concrete package-manager commands; successful wrapper updates point to the separate toolchain update check. Wrapper-owned errors use the CLI's human-readable `error:` convention, and management commands produce no entry-point return-value output.

Run `python3 tools/wrapper-e2e/run` to compare both implementations and both self-update variants. Release workflows pack candidates for every wrapper platform from the same run's compiler and pack-host artifacts, verify their embedded release fingerprint, and gate the release on conformance. Candidate archives remain separate from the shipping Rust wrapper archives until promotion. The wrapper version is intentionally unchanged; this work does not publish a release.

Both wrappers implement a separate wrapper telemetry pipeline, with `wrapper_invocation`, `wrapper_resolution`, `wrapper_install`, `wrapper_selection`, `wrapper_update`, and `wrapper_self_update` events. They share `$BAML_HOME/wrapper-telemetry.toml` and `$BAML_HOME/wrapper-telemetry/`; `DO_NOT_TRACK`, a disabled wrapper preference, or the disabled global CLI telemetry preference suppress collection and delivery. Events contain coarse wrapper metadata and bounded decisions/outcomes, without raw selectors, paths, arguments, or errors. Delivery occurs only in a detached `__flush-wrapper-telemetry` helper; Rust seals and spawns it before `exec()`. The BAML build derives ingestion constants from the independent Rust wrapper module. See [TELEMETRY.md](../../TELEMETRY.md) for fields and opt-out behavior.
