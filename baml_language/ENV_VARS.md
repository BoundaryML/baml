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
| `BAML_LOG` | Level for `log.*` output and bridge loader messages: `off`, `error`, `warn`, `info` (default), `debug`, `trace`. |
| `BAML_TELEMETRY` | Runtime tracing level: `off`, `low`, `medium` (default), `high`. Set before runtime initialization; generated artifacts may use publisher-controlled settings (see below). |
| `BOUNDARY_API_URL` | Boundary API gateway for login, cloud queries and telemetry. Overrides `[boundary].api_url`; default `https://api.prod.bcs.boundaryml.com`. |
| `BOUNDARY_API_KEY` | Non-interactive Boundary credential for cloud queries and telemetry. Takes precedence over saved user login. |
| `BOUNDARY_PROJECT` | Cloud target as `org_handle/project_name`. Overrides `[boundary].project`; query's `--project` flag takes precedence. |
| `BAML_CLI_ALLOW_DIRECT` | Hides the "don't run baml-cli directly" warning. The wrapper sets it. |
| `BAML_FEEDBACK_HOST` | Where `baml feedback` sends reports. Staging and tests. |
| `BAML_AWS_CREDENTIAL_PROCESS` | Allow running an AWS profile's `credential_process`. Off by default because it runs a program. |

## Disabling runtime tracing and local recordings

For a runtime initialized from source, set `BAML_TELEMETRY=off` before `initializeRuntime(...)` (Node) or the corresponding runtime constructor in another SDK:

```sh
BAML_TELEMETRY=off node app.mjs
```

The setting is read once per engine. `off` disables all runtime telemetry, including explicit function capture policies: no telemetry clock, processing or delivery workers, local recording directories, profiling lockfiles, captured arguments or prompts, or cloud telemetry publishing. Request building and response parsing still work. Changing the variable after initialization does not reconfigure the existing engine, and disabling tracing does not delete existing recordings. Set it before importing a generated SDK that initializes its runtime on import; in Node, setting `process.env` before a static `import` in the same module is insufficient because imports run first.

In 0.22.2 and current canary, source-initialized SDK runtimes default to `medium` tracing in memory. Without a Boundary API key or saved Boundary login, they do not write local recordings or publish to the cloud. `BOUNDARY_API_KEY=local` selects local recordings under `BAML_HOME/btel/{recordings,cas}` (default `~/.baml/btel/{recordings,cas}`); a Boundary API key or saved login selects cloud delivery. `BAML_TELEMETRY=off` suppresses either delivery path. `low` is a tracing level, not a global opt-out.

Generated bridges and packed binaries can carry a publisher-owned telemetry policy. When present, it replaces the ambient runtime settings: omission or `false` under `[bridge.env_var_names]` / `[pack.env_var_names]` keeps the baked recording level (default `medium`), even if `BAML_TELEMETRY=off` is set. To allow the standard opt-out, the publisher must generate with:

```toml
[bridge.env_var_names]
BAML_TELEMETRY = "BAML_TELEMETRY"
```

Use `[pack.env_var_names]` for packed binaries. A custom binding such as `BAML_TELEMETRY = "ACME_TELEMETRY"` instead reads `ACME_TELEMETRY=off`. Regenerate/repack after changing these settings. These artifacts default to local recordings under the BAML home when no cloud credential is selected; embedded build credentials can select cloud delivery. An honored `off` disables both local recording and cloud delivery, including delivery using an embedded build credential. Bytecode loaded without an artifact telemetry policy uses the ordinary runtime environment settings.

### Older releases: 0.20.1

`@boundaryml/baml-bridge@0.20.1` uses the previous profiler, which defaults to writing `<process.cwd()>/.baml/profiles-v1` even for initialization and build-request/parse-only usage. `BAML_TELEMETRY=off` does **not** disable that profiler. Use `BAML_PROFILE=0` before the first runtime initialization in the process, or upgrade to the current runtime. The old switch and profile session are process-global, so changing them after initialization is too late.

In that version, `BAML_PROFILE_DIR` changes the profile store path; values such as `off`, `none` and `0` are directory names. An empty value prevents the old store from opening, but is not the profiling opt-out contract; use `BAML_PROFILE=0`. `BAML_PROFILE_PUBLISH_INTERVAL_MS` controls batching of writes into the **local** store (default 1000 ms), and `publish.lock` coordinates local store publication across processes. These names do not mean profiles are uploaded. Current SDK runtime tracing no longer uses these `BAML_PROFILE*` settings or the `profiles-v1` store.

CLI usage analytics are separate: use `DO_NOT_TRACK=1` (or `baml telemetry disable`). It does not disable SDK runtime tracing or recordings, and current `BAML_TELEMETRY=off` does not opt out of CLI usage analytics. Set both when disabling both systems.

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
