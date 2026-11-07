# Environment variables

## Naming

- `BAML_*` for things users set. `BAML_BRIDGE_*` for anything every bridge shares, `BAML_BRIDGE_<LANG>_*` for one bridge.
- `DEV_BAML_<COMPONENT>_*` for dev-only knobs. Users never need these.
- Standard vars (`NO_COLOR`, `DO_NOT_TRACK`, `AWS_*`) keep their own names.
- Prefer a CLI flag. Add an env var only if it has to be set before user code runs (bridge loading), or tools that spawn the CLI need it.
- Booleans: `1/true/yes/on` or `0/false/no/off`. Empty means unset. A bad value is an error.
- Order: flag, then env, then project config, then default.

## Where state lives

BAML files are under `BAML_HOME` (default `~/.baml`). Following the [AWS login-cache pattern](https://docs.aws.amazon.com/sdkref/latest/guide/feature-login-credentials.html), each Boundary API endpoint has a JSON login file at `login/cache/<sha256-of-canonical-endpoint>.json`, shared by native CLI, SDK and packed hosts. It contains the persistent session credential and caller profile; temporary access tokens remain in process memory. Login writes atomically, and logout removes the file after attempting server revocation.

Login files are plaintext and private to the OS user: directory `0700` and file `0600` on Unix, owner-only ACLs on Windows. Other processes running as the same OS user can read them. Missing files mean no saved login; unreadable or malformed files produce an error that identifies the path.

Windows keeps an empty `login/cache/<endpoint-hash>.lock` beside each credential file to coordinate reads, atomic replacements and logout across processes. It contains no credentials and remains after logout; no lock is held during network requests.

```
~/.baml/
├── config.toml, state.toml       wrapper
├── creds.json, feedback.json     anonymous feedback state (no login credentials)
├── login/cache/                  shared Boundary login cache
│   └── <endpoint-hash>.json       saved login for one endpoint
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
| `BAML_LOG` | Level for `log.*` output from `baml run` and `baml test` (same as `--log`): `off`, `error`, `warn`, `info` (default), `debug`, `trace`. |
| `BAML_TELEMETRY` | Runtime tracing level: `off`, `low`, `medium` (default), `high`. Set `off` to disable recording and cloud uploads in CLI runs/tests and ordinary generated SDKs, including when logged in. See [Runtime traces](#runtime-traces). |
| `BOUNDARY_API_URL` | Boundary API gateway for login, cloud queries and telemetry. Overrides `[boundary].api_url`; default `https://api.prod.bcs.boundaryml.com`. |
| `BOUNDARY_API_KEY` | Non-interactive Boundary credential for cloud queries and telemetry. Takes precedence over saved user login. The sentinel `local` selects local recordings instead of cloud uploads. |
| `BOUNDARY_PROJECT` | Cloud target as `org_handle/project_name`. Overrides `[boundary].project`; query's `--project` flag takes precedence. |
| `BAML_CLI_ALLOW_DIRECT` | Hides the "don't run baml-cli directly" warning. The wrapper sets it. |
| `BAML_FEEDBACK_HOST` | Where `baml feedback` sends reports. Staging and tests. |
| `BAML_AWS_CREDENTIAL_PROCESS` | Allow running an AWS profile's `credential_process`. Off by default because it runs a program. |

## Runtime traces

Set `BAML_TELEMETRY=off` before starting your process to disable runtime recording, including cloud upload preparation and heartbeats. CLI `run`/`test` and ordinary generated SDKs honor it even when `baml auth status` reports a saved login or `BOUNDARY_API_KEY` contains a cloud credential. It creates no local trace files either. The native runtime reads the setting when it initializes; for Node, set it before importing `baml_sdk`.

```sh
BAML_TELEMETRY=off baml run main
BAML_TELEMETRY=off baml test
BAML_TELEMETRY=off node app.js
```

`0`, `false`, and `no` also disable recording; values are case-insensitive. Empty means unset, and `1`, `true`, `yes`, and `on` keep the default `medium` level. `none` and `disabled` are not supported spellings: the CLI reports an invalid setting and an ordinary generated SDK disables recording for unrecognized values.

To keep traces locally instead, set `BOUNDARY_API_KEY=local` and leave recording enabled. This bypasses cloud credentials and endpoint validation, even when logged in. CLI runs/tests write to `<project>/.baml/btel`; SDKs use `BAML_HOME/btel` when `BAML_HOME` is set, otherwise the BAML project enclosing the working directory, falling back to `~/.baml/btel` outside a project. `baml query --local` reads local recordings. `BAML_HOME` chooses a state directory; it does not disable cloud uploads.

When recording is enabled, an explicit API key or saved login still selects cloud delivery. `BAML_TELEMETRY=medium`/`high` controls recording detail, not the destination. Runtime traces and CLI usage analytics are separate: use `DO_NOT_TRACK=1` or `baml telemetry disable` for usage analytics.

Publisher-owned artifacts can explicitly freeze or rename the recording variable. `[bridge.env_var_names] BAML_TELEMETRY = false` freezes a generated SDK's recording level, and a string names its accepted variable. An explicit destination binding that reuses the `BAML_TELEMETRY` name also takes precedence over the implicit recording switch. Bridges built with `--embed-telemetry`, and packed programs, keep their baked recording level unless the publisher explicitly binds a recording variable. For those artifacts, use the publisher's declared control; see [artifact telemetry policy](crates/bcs_api/README.md#embedded-telemetry-in-artifacts).

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
| `DEV_BAML_TELEMETRY_DIAGNOSTICS_FILE` | Absolute path; each engine appends one JSON line of telemetry diagnostics at shutdown. Needs the `telemetry_diagnostics` feature. |
| `DEV_BAML_PLAYGROUND_DIR`, `DEV_BAML_PLAYGROUND_PORT` | Serve the playground from a build or a dev server. |

## Standard variables we read

`DO_NOT_TRACK` (the CLI analytics opt-out), `CLICOLOR`, `CLICOLOR_FORCE`, `CI`, `HOME`, `USERPROFILE`, `PATH`, `SHELL`, `DISPLAY`, `WAYLAND_DISPLAY`, `SSH_CONNECTION`, `SSH_TTY`, the coding-agent markers (`CLAUDECODE`, `AGENT`, `AI_AGENT`, `CODEX_SANDBOX`, `CURSOR_TRACE_ID`, `OPENCODE_CLIENT`, `PI_CODING_AGENT`, `REPL_ID`), `AWS_*`, `GOOGLE_*`, `GCLOUD_*`, `CLOUDSDK_*`.

`NO_COLOR` is honored through the `console` crate, not read by BAML code.

`NAPI_RS_NATIVE_LIBRARY_PATH`, `NAPI_RS_FORCE_WASI` and `NAPI_RS_ENFORCE_VERSION_CHECK` belong to the generated napi-rs loader, not to us.

Not listed: test, benchmark and CI-only variables, Ruby, and the old `engine/`.
