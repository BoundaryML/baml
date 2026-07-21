# BEPv2 executable reference

This is an isolated BAML package for the BEPv2 library and user-guide
scenarios. It deliberately does not depend on `crates/baml_builtins` or the
older `crates/baml_tests/baml_src/ns_ai` experiment.

## Layout

- `ns_ai/` implements the proposed shared provider, task, driver, tool,
  transcript, resource, observability, reliability, and harness contracts.
- `ns_ai/ns_drivers/` is the executable spelling of `ai.drivers.*`.
- `ns_ai_scenarios/` mirrors the `_plan/bepv2/pages/guide-*/` chapters and uses
  one support-ticket domain model throughout.
- `_plan/bepv2/_internal/deviations.md` records every known place where ordinary BAML
  cannot yet spell the normative design exactly.

## Manual desugaring convention

Compiler desugaring is intentionally absent. A future source declaration such
as `ResolveTicket.task(...)` is represented by the hand-written
`ResolveTicket_task(...)`; the corresponding direct call is represented by
`ResolveTicket_manual(...)`, which calls `root.ai.drivers.drive(...)`.
Comments above each helper state that relationship explicitly.

## Validation phases

Compile only:

```sh
target/debug/baml-cli check --from crates/baml_tests/baml_src_temp
```

Live OpenAI/Anthropic testsets are declared with the `integ-test-` prefix but
are not run during this phase. OpenAI request/stream/tool/harness tests use
`gpt-5.6-luna`; the WebSocket realtime conformance declaration uses the
endpoint-specific `gpt-realtime` model.
