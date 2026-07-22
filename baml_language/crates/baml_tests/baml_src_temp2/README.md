# BEPv2 runner-oriented comparison

This package is a source-only copy of the BEPv2 executable reference adapted to
compare nominal runner values with the function-driven baseline in
`../baml_src_temp`. Provider behavior, domain fixtures, and assertions remain
the same so API shape is the independent variable.

## Layout

- `ns_ai/` implements the proposed shared provider, task, runner, executable
  tool, conversation, resource, observability, reliability, and harness contracts.
- `ns_ai/ns_drivers/` remains the executable function-driven plumbing.
- `ns_ai/ns_run/` implements the experimental `ai.run.*` nominal classes.
- `ns_ai_scenarios/` mirrors the `_plan/bepv2/pages/guide-*/` chapters and uses
  one support-ticket domain model throughout.
- `_plan/bepv2/_internal/deviations.md` records every known place where ordinary BAML
  cannot yet spell the normative design exactly.

## Manual desugaring convention

Compiler desugaring is intentionally absent. A future source declaration such
as `ResolveTicket.task(...)` is represented by the hand-written
`ResolveTicket_task(...)`; the corresponding direct call is represented by
`ResolveTicket_manual(...)`, which calls `root.ai.drivers.complete(...)`.
Comments above each helper state that relationship explicitly.

The comparison begins after task construction:

```baml
let task = ResolveTicket_task(ticket, provider)
let response = task.run(
  runner = root.ai.run.GenerationWithMeta<Resolution>.new(),
)
```

Configured lifecycles expose their immutable state directly:

```baml
let outcome = task.run(
  runner = root.ai.run.Agent<Resolution>.new(
    tools = [knowledge_tool()],
    budget = root.ai.Budget { max_steps: 8, max_cost_usd: null },
  ),
)
```

Every purpose-built runner declares its fields and inline `Runner`
implementation in the same class. Factories provide default function
arguments and always construct complete class values.

See `_plan/bepv2/_internal/runner-provider-responsibilities.md` for the current
ownership model, API direction, and validation invariants. The earlier
`driver-functions-vs-interface-values.md` note remains the historical decision
record.

## Validation phases

Compile only:

```sh
target/debug/baml-cli check --from crates/baml_tests/baml_src_temp2
```

Run the seed offline comparison:

```sh
target/debug/baml-cli test --from crates/baml_tests/baml_src_temp2 \
  -i '::interface runner e2e*'
```

Live OpenAI/Anthropic testsets are declared with the `integ-test-` prefix but
are not run during this phase. OpenAI request/stream/tool/harness tests use
`gpt-5.6-luna`; the WebSocket realtime conformance declaration uses the
endpoint-specific `gpt-realtime` model.
