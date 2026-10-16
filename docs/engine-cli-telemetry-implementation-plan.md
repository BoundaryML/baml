# Engine CLI Telemetry Implementation Plan

Status: Proposed  
Scope: Engine CLI only (`/engine/cli`), loosely coupled, no VSCode refactor  
Last updated: 2026-02-16

## 1. Executive Summary

This document defines the exact implementation plan for adding PostHog telemetry to the current Engine CLI path used by existing BAML users.

The implementation is intentionally minimal and low-entropy:

1. Add one telemetry module in `engine/cli`.
2. Add one call site in `run_cli(...)`.
3. Emit one event per command invocation start.
4. Exclude `lsp` command.
5. Support only one env switch: `BAML_CLI_DISABLE_TELEMETRY`.
6. Keep VSCode telemetry untouched.

No shared abstraction across Rust and TypeScript is introduced.  
No VSCode telemetry API/events/settings are modified.

## 2. Background and Context

### 2.1 Why this exists

Telemetry exists for the VSCode extension today, but not for CLI invocations. As more BAML usage is initiated by non-human workflows and wrappers, CLI visibility is needed to understand adoption and behavior.

### 2.2 Current telemetry situation

1. VSCode telemetry exists in:
   - `typescript/apps/vscode-ext/src/telemetryReporter.ts`
2. Engine CLI currently has no PostHog capture.
3. Engine CLI is the real runtime path for current users (native and wrapper invocations).

### 2.3 CLI entrypoint convergence

All current-user CLI surfaces converge into:

- `engine/cli/src/lib.rs` -> `run_cli(argv, caller_type)`

Callers include:

1. Native binary:
   - `engine/cli/src/main.rs`
2. Python:
   - `engine/language_client_python/src/lib.rs`
3. TypeScript:
   - `engine/language_client_typescript/src/lib.rs`
4. Ruby:
   - `engine/language_client_ruby/ext/ruby_ffi/src/lib.rs`
5. CFFI/Go path:
   - `engine/language_client_cffi/src/ffi/runtime.rs`
   - `baml-cli/main.go`

This makes one module + one call site sufficient for broad CLI coverage.

## 3. Scope and Non-Goals

### 3.1 In scope

1. `engine/cli` telemetry only.
2. Non-`lsp` command start event capture.
3. Fire-and-forget transport.
4. One opt-out env var.
5. Unit tests for telemetry module behavior.
6. Documentation update for CLI telemetry behavior and env control.

### 3.2 Out of scope

1. Any refactor to VSCode telemetry architecture.
2. Any changes under `baml_language`.
3. Completion/success/failure lifecycle telemetry.
4. New CLI flags for telemetry.
5. Multi-env telemetry configuration surface (`host`, `mode`, `event`, etc.).

## 4. Design Constraints

1. Loose coupling: telemetry logic isolated inside one CLI module.
2. No user-impact risk: telemetry must never change command behavior or exit code.
3. Privacy-safe defaults:
   - no raw filesystem paths
   - no raw argv values
   - no arbitrary env value export
4. Minimal surface area:
   - one env var only
5. Operational simplicity:
   - fixed event name
   - fixed host/endpoint
   - fixed timeout

## 5. Event Contract (Fixed)

### 5.1 Event name

`baml.engine_cli.command.started`

### 5.2 Endpoint

- Host: `https://us.i.posthog.com`
- Path: `/i/v0/e`

### 5.3 API key

Default: same PostHog project key currently used in VSCode telemetry code.  
This is data-plane alignment only. There is no code-level coupling to VSCode telemetry runtime.

### 5.4 Payload shape

Top-level JSON payload:

```json
{
  "api_key": "<posthog_project_key>",
  "event": "baml.engine_cli.command.started",
  "distinct_id": "<machine_id>",
  "$process_person_profile": false,
  "properties": {
    "surface": "engine_cli",
    "schema_version": 1,
    "cli_version": "0.0.0",
    "command": "generate",
    "subcommand": null,
    "caller_output_type": "typescript",
    "caller_runtime": "typescript",
    "ci": false,
    "ci_provider": "none",
    "project_hash": "ab12cd34",
    "project_hash_source": "from_arg",
    "machine_id": "baml_machine_....",
    "session_id": "uuid-v4",
    "argv_len": 3,
    "feature_flags_count": 1,
    "os_platform": "macos",
    "os_arch": "aarch64",
    "stdout_is_tty": true,
    "stderr_is_tty": true
  }
}
```

## 6. Configuration Surface (Final)

### 6.1 Supported env var

`BAML_CLI_DISABLE_TELEMETRY`

Behavior:

1. Truthy values disable telemetry hard.
2. Falsy/unset values allow telemetry.

Truthy parser set:

- `1`
- `true`
- `yes`
- `on`

Case-insensitive, surrounding whitespace trimmed.

### 6.2 Explicitly not added

The following are intentionally not implemented:

1. `BAML_CLI_TELEMETRY_MODE`
2. `BAML_CLI_TELEMETRY_API_KEY`
3. `BAML_CLI_TELEMETRY_HOST`
4. `BAML_CLI_TELEMETRY_EVENT_NAME`
5. `BAML_CLI_TELEMETRY_TIMEOUT_MS`
6. `BAML_CLI_TELEMETRY_PROCESS_PERSON_PROFILE`
7. `BAML_CLI_TELEMETRY_DEBUG`

Reason: keep loose coupling and reduce entropy of configuration behavior.

## 7. Data Collection Rules

### 7.1 Command capture

Capture only start event for commands except:

1. `lsp` (always excluded)

### 7.2 Command and subcommand mapping

Map from `commands::Commands`:

1. `Init` -> `init`
2. `Generate` -> `generate`
3. `Check` -> `check`
4. `Serve` -> `serve`
5. `Dev` -> `dev`
6. `Auth(Login|Token)` -> `auth` with `subcommand=login|token`
7. `Login` -> `login`
8. `Deploy` -> `deploy`
9. `Format` -> `fmt`
10. `Test` -> `test`
11. `DumpHIR` -> `dump_hir`
12. `DumpBytecode` -> `dump_bytecode`
13. `Repl` -> `repl`
14. `Optimize` -> `optimize`
15. `LanguageServer` -> excluded

### 7.3 Caller mapping

Derive `caller_output_type` from `RuntimeCliDefaults.output_type`.  
Derive `caller_runtime` from output type mapping:

1. `OpenApi` -> `native`
2. `PythonPydantic` / `PythonPydanticV1` -> `python`
3. `Typescript` / `TypescriptReact` -> `typescript`
4. `RubySorbet` -> `ruby`
5. `Go` -> `go`
6. fallback -> `unknown`

### 7.4 Project hash derivation

Path selection order:

1. `--from <path>` from raw argv if present.
2. `cwd/baml_src` if it exists.
3. `cwd`.

Then:

1. normalize path string
2. SHA-256 hash
3. truncate to 8 hex chars

Only hash is sent, never raw path.

### 7.5 CI metadata

`ci` is true when `CI` env is truthy.  
`ci_provider` inferred by first match:

1. `GITHUB_ACTIONS` -> `github_actions`
2. `GITLAB_CI` -> `gitlab`
3. `CIRCLECI` -> `circleci`
4. `BUILDKITE` -> `buildkite`
5. `JENKINS_URL` -> `jenkins`
6. `TF_BUILD` -> `azure_pipelines`
7. otherwise -> `none`

## 8. Reliability and Failure Semantics

### 8.1 Delivery strategy

Fire-and-forget only:

1. Spawn detached thread.
2. In thread, create short-lived tokio runtime.
3. Use `reqwest` POST to PostHog endpoint.
4. Apply fixed timeout: 800ms.
5. Ignore all errors.

### 8.2 Guarantees

1. Telemetry failure cannot alter stdout/stderr contract.
2. Telemetry failure cannot alter exit code.
3. Telemetry cannot panic process path.

## 9. Privacy and Security

1. No raw argv values are exported.
2. No raw filesystem path values are exported.
3. No sensitive env values are exported.
4. Distinct ID is anonymous machine ID.
5. `$process_person_profile=false`.

## 10. File-Level Implementation Plan

### 10.1 New file

`engine/cli/src/telemetry.rs`

Contents:

1. Config parsing (`BAML_CLI_DISABLE_TELEMETRY`).
2. Command mapping utilities.
3. Project hash selection and hashing.
4. Machine ID persistence logic.
5. CI detection utilities.
6. Payload construction structs.
7. Async HTTP capture helper.
8. Public entry function called by `run_cli(...)`.

### 10.2 Existing file edit

`engine/cli/src/lib.rs`

Changes:

1. `mod telemetry;`
2. After `parse_from_smart(argv.clone())` and before command execution, call:
   - `telemetry::capture_command_started(&argv, &cli.command, caller_type);`
3. No changes to command behavior flow.

### 10.3 Docs file add

`fern/03-reference/baml-cli/telemetry.mdx`

Contents:

1. What is captured.
2. What is not captured.
3. How to disable: `BAML_CLI_DISABLE_TELEMETRY`.
4. Privacy guarantees.

### 10.4 Docs navigation update

`fern/docs.yml`

Add telemetry page under `baml-cli` section.

## 11. Proposed Internal API (telemetry.rs)

Suggested shape:

```rust
pub(crate) fn capture_command_started(
    argv: &[String],
    command: &crate::commands::Commands,
    caller_type: baml_runtime::RuntimeCliDefaults,
)
```

Supporting internal items:

1. `struct TelemetryEvent`
2. `struct TelemetryProperties`
3. `enum ProjectHashSource`
4. `enum CiProvider`
5. `fn env_truthy(key: &str) -> bool`
6. `fn telemetry_disabled() -> bool`
7. `fn is_lsp_command(command: &Commands) -> bool`
8. `fn map_command(command: &Commands) -> (&'static str, Option<&'static str>)`
9. `fn map_caller_runtime(...) -> &'static str`
10. `fn compute_project_hash(argv: &[String]) -> (String, ProjectHashSource)`
11. `fn get_or_create_machine_id() -> String`
12. `async fn send_event(payload: &TelemetryEvent) -> anyhow::Result<()>`

## 12. Machine ID Persistence Plan

Use app config strategy similar to existing CLI credential storage patterns.

Directory strategy:

1. top-level-domain: `com`
2. author: `boundaryml`
3. app-name: `baml-cli`

File:

`<config_dir>/telemetry_machine_id`

Behavior:

1. Read if exists and non-empty.
2. If missing/unreadable/empty:
   - generate UUID v4
   - attempt write
   - use generated value even if write fails

## 13. Test Plan

Unit tests in `engine/cli/src/telemetry.rs` (or companion test module):

1. `env_truthy` parser:
   - truthy set passes
   - falsy/unset fails
2. disable behavior:
   - disabled env suppresses send
3. lsp exclusion:
   - `LanguageServer` command never sends
4. command mapping:
   - all commands map to expected `command`/`subcommand`
5. project hash source fallback:
   - `--from` path
   - `cwd/baml_src`
   - `cwd`
6. CI provider mapping:
   - each provider env produces expected enum/name
7. payload shape:
   - required fields populated
   - no raw path/argv in serialized payload

Non-goal for tests:

1. No live network integration test with PostHog endpoint.

## 14. Acceptance Criteria

1. Any non-`lsp` CLI invocation emits one start event when not disabled.
2. `baml-cli lsp` emits no telemetry.
3. `BAML_CLI_DISABLE_TELEMETRY=1` disables all CLI telemetry.
4. Network failures/timeouts do not alter command behavior or exit code.
5. Payload never includes raw path or argv strings.
6. VSCode telemetry behavior remains unchanged.

## 15. Rollout and Verification

### 15.1 Rollout

Single-release rollout with no feature flag beyond env disable switch.

### 15.2 Verification checklist

1. Manual run:
   - `baml-cli generate`
   - confirm send path executed
2. Manual run:
   - `BAML_CLI_DISABLE_TELEMETRY=1 baml-cli generate`
   - confirm no send path
3. Manual run:
   - `baml-cli lsp`
   - confirm no send path
4. Confirm VSCode extension telemetry code unchanged in diff.

## 16. Risk Register

1. Risk: unintentional latency from network call.
   - Mitigation: detached thread + short timeout + no await in main path.
2. Risk: accidental data leakage.
   - Mitigation: explicit allowlist-only payload construction.
3. Risk: storage failures for machine id.
   - Mitigation: fallback to ephemeral UUID.
4. Risk: future config sprawl.
   - Mitigation: single env var contract; reject extra knobs for this iteration.

## 17. Explicit Change Approval List

The implementation of this plan introduces exactly these repo-level changes:

1. Add `engine/cli/src/telemetry.rs`
2. Edit `engine/cli/src/lib.rs`
3. Add `fern/03-reference/baml-cli/telemetry.mdx`
4. Edit `fern/docs.yml`

No other files are required for the planned implementation.

## 18. Appendix: Coupling Clarification

This plan is loosely coupled by design:

1. CLI telemetry is implemented entirely in Rust under `engine/cli`.
2. VSCode telemetry remains separate in TypeScript.
3. No shared runtime interfaces between them.
4. Shared PostHog project key is only a data destination choice, not code coupling.

