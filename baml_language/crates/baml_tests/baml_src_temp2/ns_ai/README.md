# `ai` reference library

Ordinary directories divide files by responsibility without creating extra
BAML namespaces. `ns_drivers/` and its `ns_unsafe/` child retain the baseline
`ai.drivers` plumbing. `ns_driver/` creates the experimental singular
`ai.driver` namespace containing nominal driver factories.

- `core/`: task, provider, response, messages, transcripts, media streams
- `providers/`: shared wire bridge plus OpenAI and Anthropic adapters
- `tools/`: tool schemas, registries, hooks, outcomes, and the agent loop
- `reliability/`: replay policy, retry, fallback, and routing
- `resources/`: background jobs, batches, caches, sessions, realtime channels
- `observability/`: provider-neutral events and usage accounting
- `harness/`: external-runtime sessions and a real-model harness adapter
- `testing/`: deterministic generation and tool providers
- `ns_driver/`: immutable interface-driven lifecycle values used by scenarios

Concrete provider capabilities are attached with out-of-body `implements`
blocks wherever that makes extension without declaration ownership visible.
