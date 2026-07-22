# `ai` reference library

Ordinary directories divide files by responsibility without creating extra
BAML namespaces. `ns_drivers/` and its `ns_unsafe/` child retain low-level
executable plumbing. `ns_run/` creates the public `ai.run` namespace containing
nominal runner classes.

- `core/`: task, runner, provider, response, messages, conversations, media streams
- `providers/`: shared wire bridge plus OpenAI and Anthropic adapters
- `tools/`: AnyFunction-backed tools, registries, hooks, outcomes, and agent loop
- `reliability/`: replay policy, retry, fallback, and routing
- `resources/`: background jobs, batches, caches, sessions, realtime channels
- `observability/`: provider-neutral events and usage accounting
- `harness/`: external-runtime sessions and a real-model harness adapter
- `testing/`: deterministic generation and tool providers
- `ns_run/`: configured lifecycle values used by scenarios

Each purpose-built runner keeps its configuration fields and inline
`implements Runner<...>` block together. Factory methods use default function
arguments to return fully initialized values; BAML class fields do not have
defaults. Tasks call `task.run(runner = ...)`, while plural/provider/media
inputs call `runner.run(input)` directly.

`ai.run.Agent` owns budgets, hooks, observers, the active tool registry, and an
optional resumable provider `Conversation`. Application `Tool` values retain
their original `baml.AnyFunction` handlers, so the Agent invokes them through
`reflect.call_any` without a parallel name-switch dispatcher.

Concrete provider capabilities are attached with out-of-body `implements`
blocks wherever that makes extension without declaration ownership visible.
