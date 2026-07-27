# BEPv2 Reconciliation Task List

This checklist reconciles the normative BEPv2 documents with the executable
reference under `crates/baml_tests/baml_src_temp`. A checked item must have a
compiling implementation and an executable scenario; documentation-only
agreement is not sufficient.

## Canonical decisions

- [x] Keep the BEP's task/driver/provider/resource split. The reference may use
  runtime capability matches only where `Task<T, P>` cannot yet be expressed.
- [x] Rename the reference suite's `fake(output)` fixture to
  `fake_output_provider(output)`. This is private test support, not a proposed
  public `ai` API.
- [x] Keep the symmetric driver surface: `drive` and `generate` return `T`;
  `drive_with_meta` and `generate_with_meta` return `Response<T>`.
- [x] Use the current `baml.errors.Failure` predicates as the error facts.
  Retry/fallback combine those facts with operation replay policy. Unknown or
  foreign errors are never assumed replay-safe.
- [x] Make one driver-owned mutable `ToolRegistry` the source of truth for a
  run. `StepContext` exposes only the current tool snapshot. Hooks return the
  complete next roster through `StepPlan.tools`; the driver validates and
  atomically applies it. Duplicate names fail visibly.
- [x] Keep application dispatch separate until BEP-062 function values land,
  but require typed argument decoding before application handlers run.
- [x] Treat a harness as an execution owner passed to `submit_harness`, not as
  the task's model provider. A harness adapter may separately implement
  `Provider`/`DriveProvider`, but harness execution does not require that.
- [x] Keep provider-owned sessions separate from transcript persistence:
  `SessionProvider` opens/resumes `Session`; resumable tool providers
  save/restore `Transcript`.
- [x] Give every provider-owned resource an idempotent `cleanup()` operation;
  domain verbs such as `cancel`, `close`, and `delete` may remain aliases.
- [x] Treat the exact direct `cleanup(self) -> null throws never` method as an
  at-most-once GC finalizer. Use `defer` for deterministic production cleanup
  and `baml.sys.collect_garbage()` only for tests and runtime diagnostics.
- [x] Construct providers, tasks, hooks, and options atomically with exact-class
  spread or named factory parameters. Reserve mutation for explicitly stateful
  registries, transcripts, resources, counters, and event buffers.

## Implementation and scenario work

- [x] Rename the canned-output fake and update all scenarios.
- [x] Align generation driver return types and unsafe-driver counterparts.
- [x] Make retry and fallback failure-aware; test retryable, terminal, unknown,
  and replay-forbidden failures.
- [x] Fix tool-removal persistence, collision behavior, explicit replacement,
  and roster-change events.
- [x] Replace the schema-only MCP example with a deterministic two-turn test
  that discovers, advertises, validates, and dispatches a tool mid-loop.
- [x] Enforce step and cost budgets, preserve hook stop reasons, and emit usage
  and provider-change events.
- [x] Resume from a task's provider transcript without silently starting over;
  validate transcript-token provider/version ownership.
- [x] Implement provider-owned session drivers and lifecycle tests.
- [x] Separate harness submission from the task provider and update live tests.
- [x] Add idempotent cleanup coverage for jobs, batches, sessions, live
  channels, caches, and harness sessions.
- [x] Add an explicit full-GC builtin and a regression proving that it drains
  unreachable resource finalizers before returning.
- [x] Make the bounded-audio scenario prove that audio reaches the selected
  transcription capability rather than only counting chunks in a prompt.
- [x] Replace construct-then-assign scenario setup with typed class spread and
  named `guide_agent_options` parameters.
- [x] Type-check class-spread operands as ordinary expressions, require the
  destination's nominal class and generic arguments, and cover omitted-default
  factory calls through both the VM and parallel test runner.

## Verification gates

- [x] Fresh `baml-cli check` succeeds for `baml_src_temp`.
- [x] All non-`integ-test-*` tests pass from a fresh CLI build.
- [x] Each live integration testset is independently selectable and has an
  assertion on the capability it claims to cover.
- [x] Authorized live provider testsets pass, with credential or provider
  failures reported separately from offline failures. On 2026-07-17, the full
  `integ-test-*` selection passed 57/57 tests through Infisical in 19 seconds.
- [x] `_internal/deviations.md`, the API reference, and user-guide examples match
  the final executable surface.
