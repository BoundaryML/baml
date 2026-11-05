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
| `BAML_HOME` | Move user state (CI workspace, container, read-only home). Also selects the local trace store unless the project sets `[tracing].store`; see below. Empty means unset. |
| `BAML_TOOLCHAIN` | Which toolchain `baml` runs: a channel, a version, or a path to a local `baml-cli`. Default `canary`. |
| `BAML_MANIFEST_BASE_URL` | Release manifest mirror, used by the wrapper and the bridges. |
| `BAML_BUILD_CACHE` | `false` turns off the build cache. |
| `BAML_BUILD_CACHE_REMOTE`, `BAML_BUILD_CACHE_REMOTE_TOKEN` | Shared build cache for CI. URL must be https (or localhost). |
| `BAML_LOG` | Level for `log.*` output from `baml run` and `baml test` (same as `--log`): `off`, `error`, `warn`, `info` (default), `debug`, `trace`. |
| `BAML_TELEMETRY` | Runtime tracing level: `off`, `low`, `medium` (default), `high`. |
| `BOUNDARY_API_URL` | Boundary API gateway for login, cloud queries and telemetry. Overrides `[boundary].api_url`; default `https://api.prod.bcs.boundaryml.com`. |
| `BOUNDARY_API_KEY` | Non-interactive Boundary credential for cloud queries and telemetry. Takes precedence over saved user login. Set to `local` to record and query locally even when a saved cloud login exists. |
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
| `DEV_BAML_TELEMETRY_DIAGNOSTICS_FILE` | Absolute path; each engine appends one JSON line of telemetry diagnostics at shutdown. Needs the `telemetry_diagnostics` feature. |
| `DEV_BAML_PLAYGROUND_DIR`, `DEV_BAML_PLAYGROUND_PORT` | Serve the playground from a build or a dev server. |

## Standard variables we read

`DO_NOT_TRACK` (the CLI analytics opt-out), `CLICOLOR`, `CLICOLOR_FORCE`, `CI`, `HOME`, `USERPROFILE`, `PATH`, `SHELL`, `DISPLAY`, `WAYLAND_DISPLAY`, `SSH_CONNECTION`, `SSH_TTY`, the coding-agent markers (`CLAUDECODE`, `AGENT`, `AI_AGENT`, `CODEX_SANDBOX`, `CURSOR_TRACE_ID`, `OPENCODE_CLIENT`, `PI_CODING_AGENT`, `REPL_ID`), `AWS_*`, `GOOGLE_*`, `GCLOUD_*`, `CLOUDSDK_*`.

`NO_COLOR` is honored through the `console` crate, not read by BAML code.

`NAPI_RS_NATIVE_LIBRARY_PATH`, `NAPI_RS_FORCE_WASI` and `NAPI_RS_ENFORCE_VERSION_CHECK` belong to the generated napi-rs loader, not to us.

Not listed: test, benchmark and CI-only variables, Ruby, and the old `engine/`.

## Shared local trace store

Set one local store for SDK bridge recordings, `baml run` / `baml test`, and `baml query --local` in the project's `baml.toml`:

```toml
[package]
name = "computer-use"

[tracing]
store = ".baml"
```

`store` names a state directory containing `btel/recordings/`, shared `btel/cas/` blobs, and the disposable `btel/query.sqlite` index. Relative paths are resolved against the directory containing `baml.toml`, including when you launch from a nested directory. Absolute paths can share a store across projects. Paths are literal: `~` and environment variables are not expanded inside TOML strings. Add the store directory to `.gitignore` if it is inside your repository.

For local recording and querying, the precedence is:

1. An explicit traces directory passed to `baml query --from` (query only).
2. The resolved project's `[tracing].store`.
3. Nonempty `BAML_HOME`.
4. `<project>/.baml` when a project is available.
5. `~/.baml` without a project (`.baml` relative to the working directory if the OS cannot provide a home directory).

The CLI uses its resolved project root, including an explicit project passed to `--from`. The SDK bridge discovers the project enclosing the host process's working directory using the same `baml.toml` / `baml_src/` markers as the CLI. A nonempty `BAML_HOME` can be relative to the process working directory; use an absolute path when launching processes from different directories. `[tracing].store` takes precedence over `BAML_HOME` for traces while authentication, build caches, and toolchains continue using `BAML_HOME`. An invalid store setting produces a configuration error; an unwritable store disables recording with a diagnostic rather than silently writing into another store.

The store setting chooses a filesystem location, independently of cloud/local destination selection and recording level. Use `BOUNDARY_API_KEY=local` to select local recording and query even with a saved Boundary login; `baml query --local` selects local reads without changing how your application records. The store setting does not enable recording that the runtime telemetry policy has disabled. This setting is read at runtime from the local manifest; it is not embedded into generated SDK artifacts. A bridge launched outside the project cannot discover that manifest: set an absolute `BAML_HOME` in both processes in that case. Packed executables always use `BAML_HOME` (otherwise the user store), so launching one inside a project does not adopt that project's store.

For example, launch your computer-use demo from its project and query its recordings with the same environment. CLI entrypoints use this store too (`baml run main` below assumes the project defines `main`):

```sh
cd /path/to/demo-computer-use
export BOUNDARY_API_KEY=local
export BAML_TELEMETRY=high
# Run your application using baml_bridge from this project or a child directory.
baml run main
baml query --local 'SELECT COUNT(*) FROM processes'
```

`baml query --from` also accepts an existing state directory, a directory containing `recordings/` and `cas/`, or the `recordings/` directory itself. A recognized traces directory selects local querying directly, even with cloud credentials set. It takes precedence over enclosing project configuration and `BAML_HOME`; it cannot be combined with cloud `--project` or `--environment` selectors. An explicit project root uses its project settings even if it contains unrelated `btel/` or `recordings/` directories. These commands all read the same store:

```sh
baml query --from /path/to/demo-computer-use/.baml 'SELECT COUNT(*) FROM processes'
baml query --from /path/to/demo-computer-use/.baml/btel 'SELECT COUNT(*) FROM processes'
baml query --from /path/to/demo-computer-use/.baml/btel/recordings 'SELECT COUNT(*) FROM processes'
# Query the global user store from anywhere, including inside a project:
baml query --from "$HOME/.baml" 'SELECT COUNT(*) FROM processes'
```

Compatibility: without `[tracing].store` or `BAML_HOME`, existing project stores keep their location. With `BAML_HOME`, CLI local writes and reads now follow the same user store as the bridge. Existing recordings are not moved or combined automatically. To inspect an older project store while `BAML_HOME` is set, pass its `.baml` directory explicitly, or add `store = ".baml"` to keep that project local.

### Querying large stores

The first query builds an index of the selected store; later refreshes reconcile completed files using metadata and decode new or changed files. Refresh still lists every recording and stats completed files, so a global store accumulating many projects has more discovery and reconciliation work than a project store. SQL `LIMIT` and `--max-wall` constrain query execution, not this preceding refresh. `--format json` exposes `outcome.refresh` counters/timings and `outcome.query` timings to distinguish those costs.

Use a per-project store to limit how much unrelated history each refresh visits. After building the index, `baml query --from /path/to/store --no-refresh 'SELECT COUNT(*) FROM processes'` skips file reconciliation and queries the existing index; it does not include recordings added since the last refresh. Queries can still scan indexed rows and report extents for all recordings, and captured-value expressions can read CAS blobs. This path/configuration change does not claim a speedup for SQL over the same global dataset; global-store query performance is being investigated separately.
