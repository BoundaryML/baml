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
| `BAML_TELEMETRY` | Runtime tracing level: `off`, `low`, `medium` (default), `high`. Set `off` to disable recording and cloud uploads in CLI runs/tests and native generated SDKs/packed programs using the standard control, including when logged in. |
| `BOUNDARY_API_URL` | Boundary API gateway for login, cloud queries and telemetry. Overrides `[boundary].api_url`; default `https://api.prod.bcs.boundaryml.com`. |
| `BOUNDARY_API_KEY` | Non-interactive Boundary credential for cloud queries and telemetry. Takes precedence over saved user login. The sentinel `local` selects local recordings instead of cloud uploads when the artifact permits this API-key binding. |
| `BOUNDARY_PROJECT` | Cloud target as `org_handle/project_name`. Overrides `[boundary].project`; `baml cloud --project` takes precedence. |
| `BAML_CLI_ALLOW_DIRECT` | Hides the "don't run baml-cli directly" warning. The wrapper sets it. |
| `BAML_AWS_CREDENTIAL_PROCESS` | Allow running an AWS profile's `credential_process`. Off by default because it runs a program. |
| `BAML_GC_GROWTH` | How much the heap may grow between collections, as a multiple of the data that survived the last one: `0.25` to `1000`, at most two decimals. Default `4`. See [Memory](#memory). |
| `BAML_GC_MIN_GROWTH` | The least the heap may grow between collections, and before the first: a size such as `8MiB` (`B`, `KiB`, `MiB`, `GiB`; a bare number is bytes), at least `1MiB`. Default `32MiB`. See [Memory](#memory). |

## Memory

BAML's heap is garbage collected. After each collection, the next one waits until the heap has grown by `growth` times the data that survived, and by at least `min_growth`. Memory in use is then roughly the live data plus the allowed growth, and a collection needs extra room while it copies the survivors. Lower values use less memory and more collector time: collector work per allocated byte is about `1 / growth`, so `growth = 1` does about four times the collector work of the default `4`, and values below `1` are allowed but costly. Programs that allocate little barely notice either setting. `min_growth` keeps small programs from collecting constantly; lower it, together with `growth`, where memory is tight (AWS Lambda, small containers). These are not a hard memory limit.

| | Default | Environment | Packed programs (`baml.toml`) |
|---|---|---|---|
| growth | `4` | `BAML_GC_GROWTH=2.5` | `[pack.gc]` `growth = 2.5` |
| min_growth | `32MiB` | `BAML_GC_MIN_GROWTH=8MiB` | `[pack.gc]` `min_growth = "8MiB"` |

- `baml run`, `baml test`, the playground and every generated SDK read the two variables when the runtime starts. `[pack.gc]` does not apply to them.
- `baml pack` bakes `[pack.gc]` into the binary. At startup the variables still win over the baked values, each setting on its own. A publisher can rename the variables the binary reads with `[pack.env_var_names]` (`BAML_GC_GROWTH = "ACME_GC_GROWTH"`), but cannot turn them off.
- A value that cannot work (`growth` below `0.25`, `min_growth` below `1MiB`, a decimal unit such as `MB`, anything that is not a number) stops startup with an error naming the variable, or fails `baml pack` naming the key. Empty means unset.
- Each running program (one engine) has one heap and one policy, shared by its spawned tasks. Several engines in one process each get their own heap and allowance.

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
