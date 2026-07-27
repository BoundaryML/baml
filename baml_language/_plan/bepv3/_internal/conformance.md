# Conformance and testing

The proposal is complete only when compiler, standard library, runtime, and
live provider behavior agree.

## Compiler

Compiler tests must cover:

- companion `.task(...)` construction for every LLM function shape;
- preservation of `T` and provider type `P`;
- `Task.run` associated `Output` and `Error` projection;
- method-call dispatch to the concrete runner without recursion;
- generic runner `implements` blocks;
- capability failures with actionable diagnostics;
- direct-call lifecycle selection from application tools;
- task provider and tool overrides;
- `AnyFunction` conversion for functions with generic inputs, defaults, and
  declared errors;
- graph identity through every standard runner; and
- no `<unknown>` in inferred public runner results.

Negative compiler tests must include incompatible providers for Agent, Stream,
Background, Batch, and live sessions.

## Standard library

Deterministic tests must cover:

- each standard runner's output and error types;
- Agent completion, budget stop, and handoff;
- tool declaration inheritance, clearing, and replacement;
- invalid tool JSON and default arguments;
- multiple and parallel tool calls;
- hooks that approve, deny, rewrite, add, and remove tools;
- dynamic MCP discovery;
- retry and fallback before side effects;
- no replay after side effects or stream yield;
- message export and conversation import fidelity;
- exact resume identity checks;
- resource close, cancellation, and cleanup;
- fake provider scripts and injected failures; and
- heterogeneous batch item result typing.

Tests should assert specific events, arguments, results, usage totals, and
terminal outcomes. A test that only checks “did not throw” is insufficient for
a documented behavior.

## Private conformance fixtures

The conformance suite may define local provider implementations that return
scripted values, tool calls, or classified failures. These fixtures belong to
test-only source and are not part of the public `ai` namespace.

Fixture names and constructors are implementation details, not API. The
fixtures must still validate the operation they receive and fail if the wrong
function, prompt, tool schema, provider phase, or continuation result is used.

## Live provider matrix

At least one live integration test must exercise each public behavior for
which a real provider can add confidence:

- completion and structured decoding;
- generation metadata;
- streaming with more than one emitted update;
- application tool call and final answer;
- multiple and parallel tool calls;
- invalid-argument recovery;
- Agent resume from a real conversation;
- provider switching through message import;
- background job submit, poll, and result;
- batch submission and typed item results;
- provider-owned hosted tool;
- media input;
- raw live session;
- voice Agent tool call;
- usage reporting; and
- provider failure facts where a safe fault can be induced.

Tests requiring credentials run through:

```console
infisical run --env=test -- baml-cli test ...
```

They must log the specific behavior under test and assert the corresponding
typed result or event. Live tests should be tagged by capability so unsupported
providers are skipped explicitly, not treated as passing.

## Cleanup tests

Resource tests use explicit close for ordinary assertions and a deterministic
runtime GC trigger for cleanup fallback assertions.

The suite must show that explicit close plus garbage collection performs the
underlying release exactly once.

## Performance

Listing tests must compile once, avoid executing tests or initializing live
providers, and return selectors promptly:

```console
baml-cli test --list
```

The integration suite should record list duration in CI so discovery
regressions are visible.

## Documentation examples

Every public example page includes a complete LLM function declaration. A
documentation check should verify that code blocks stay type-correct as the
surface evolves.

The public pages may use illustrative provider names. The conformance fixtures
must replace those names with concrete fake or live provider values and run
the corresponding scenario.
