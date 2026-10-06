# Environment variables

## Naming

- `BAML_*` for things users set. `BAML_BRIDGE_*` for anything every bridge shares, `BAML_BRIDGE_<LANG>_*` for one bridge.
- `DEV_BAML_<COMPONENT>_*` for dev-only knobs. Users never need these.
- Standard vars (`NO_COLOR`, `DO_NOT_TRACK`, `AWS_*`) keep their own names.
- Prefer a CLI flag. Add an env var only if it has to be set before user code runs (bridge loading), or tools that spawn the CLI need it.
- Booleans: `1/true/yes/on` or `0/false/no/off`. Empty means unset. A bad value is an error.
- Order: flag, then env, then project config, then default.

## Where state lives

BAML files are under `BAML_HOME` (default `~/.baml`). Each Boundary API endpoint has a login file at `auth/<sha256-of-canonical-endpoint>.json`, shared by native CLI, SDK and packed hosts. It contains the persistent session credential and caller profile; temporary access tokens remain in process memory. Login writes atomically, and logout removes the file after attempting server revocation.

Login files are plaintext and private to the OS user: directory `0700` and file `0600` on Unix, owner-only ACLs on Windows. Other processes running as the same OS user can read them. Missing files mean no saved login; unreadable or malformed files produce an error that identifies the path.

```
~/.baml/
├── config.toml, state.toml       wrapper
├── creds.json, feedback.json     anonymous feedback state (no login credentials)
├── auth/<endpoint-hash>.json     saved Boundary login
├── telemetry.toml                CLI analytics opt-out
├── toolchains/<version>/         installed baml-cli
├── manifest-cache/
├── bridges/<version>/<target>/   native library, shared by every language
└── build/cache/
```

## Variables

| Variable | What it does |
|---|---|
| `BAML_HOME` | Move all BAML state (CI workspace, container, read-only home). |
| `BAML_TOOLCHAIN` | Which toolchain `baml` runs: a channel, a version, or a path to a local `baml-cli`. Default `canary`. |
| `BAML_MANIFEST_BASE_URL` | Release manifest mirror, used by the wrapper and the bridges. |
| `BAML_BUILD_CACHE` | `false` turns off the build cache. |
| `BAML_BUILD_CACHE_REMOTE`, `BAML_BUILD_CACHE_REMOTE_TOKEN` | Shared build cache for CI. URL must be https (or localhost). |
| `BAML_LOG` | Level for `log.*` output and bridge loader messages: `off`, `error`, `warn`, `info` (default), `debug`, `trace`. |
| `BAML_TELEMETRY` | Runtime tracing level: `off`, `low`, `medium` (default), `high`. |
| `BOUNDARY_API_URL` | Boundary API gateway for login, cloud queries and telemetry. Overrides `[boundary].api_url`; default `https://api.cloud.boundaryml.com`. |
| `BOUNDARY_API_KEY` | Non-interactive Boundary credential for cloud queries and telemetry. Takes precedence over saved user login. |
| `BOUNDARY_PROJECT` | Cloud target as `org_handle/project_name`. Overrides `[boundary].project`; query's `--project` flag takes precedence. |
| `BAML_CLI_ALLOW_DIRECT` | Hides the "don't run baml-cli directly" warning. The wrapper sets it. |
| `BAML_FEEDBACK_HOST` | Where `baml feedback` sends reports. Staging and tests. |
| `BAML_AWS_CREDENTIAL_PROCESS` | Allow running an AWS profile's `credential_process`. Off by default because it runs a program. |

## Bridges

Go, Rust, C++, Java, C# and TypeScript load a native library. Python and Swift link it at build time. Only Go and Rust download it.

| Variable | What it does |
|---|---|
| `BAML_BRIDGE_PATH` | Use this exact library. No download, no fallback. |
| `BAML_BRIDGE_DISABLE_DOWNLOAD` | Never touch the network. |
| `BAML_BRIDGE_SWIFT_ALLOW_MAIN_THREAD_SYNC` | Silences the debug warning about sync calls on the main thread. |

## Dev only

| Variable | What it does |
|---|---|
| `DEV_BAML_BRIDGE_SKIP_VERSION_CHECK` | Load a local library with a different version. Needs `BAML_BRIDGE_PATH`. |
| `DEV_BAML_BUILD_CACHE_VERIFY` | `off`, `sampled` (default) or `always`: recompile and compare against the cache. |
| `DEV_BAML_CLI_DISABLE_AGENT_DETECTION` | Act as if not run by a coding agent. Set by the `mise` tasks. |
| `DEV_BAML_VM_KPERF` | Hardware counters around VM execution (Apple Silicon). |
| `DEV_BAML_QUERY_FAULT` | Crash injection for telemetry recovery tests. |
| `DEV_BAML_HEAP_VERIFY` | `off`, `quick`, `full`. Needs the `heap_debug` feature. |
| `DEV_BAML_PLAYGROUND_DIR`, `DEV_BAML_PLAYGROUND_PORT` | Serve the playground from a build or a dev server. |

## Standard variables we read

`DO_NOT_TRACK` (the CLI analytics opt-out), `CLICOLOR`, `CLICOLOR_FORCE`, `CI`, `HOME`, `USERPROFILE`, `PATH`, `SHELL`, `DISPLAY`, `WAYLAND_DISPLAY`, `SSH_CONNECTION`, `SSH_TTY`, the coding-agent markers (`CLAUDECODE`, `AGENT`, `AI_AGENT`, `CODEX_SANDBOX`, `CURSOR_TRACE_ID`, `OPENCODE_CLIENT`, `PI_CODING_AGENT`, `REPL_ID`), `AWS_*`, `GOOGLE_*`, `GCLOUD_*`, `CLOUDSDK_*`.

`NO_COLOR` is honored through the `console` crate, not read by BAML code.

`NAPI_RS_NATIVE_LIBRARY_PATH`, `NAPI_RS_FORCE_WASI` and `NAPI_RS_ENFORCE_VERSION_CHECK` belong to the generated napi-rs loader, not to us.

Not listed: test, benchmark and CI-only variables, Ruby, and the old `engine/`.
