# `root.ai` BEPv2 reference implementation

Only the outer `ns_ai/` directory contributes a BAML namespace. Every ordinary
directory below it is for source navigation, so all declarations here remain
available as `root.ai.*`.

- `core/`: provider identity, messages, requests, and responses.
- `capabilities/`: the public capability interfaces and their dispatch helpers.
- `providers/`: one directory per provider family.
- `tools/`: provider-independent tool definitions and agent-loop orchestration.
- `harness/`: external agent runtimes, exact continuation tokens, rich events,
  tool middleware, observers, and the `HarnessAgent` task facade.
- `orchestration/`: provider-neutral composition such as quality cascades and
  typed judge pipelines.
- `testing/`: deterministic providers and resources shared by numbered
  scenarios; scenarios never depend on declarations from another scenario.
- `reliability/`: failures, replay policy, retries, fallback, and tracing.

Within a provider directory, `provider.baml` owns configuration and provider
identity. Sibling files named `generate.baml`, `streaming.baml`,
`capabilities.baml`, `background.baml`, `sessions.baml`, or `capability.baml`
show the surfaces that provider implements. Provider-specific wire types live
beside the capability that consumes them—for example, OpenAI Chat Completions
tool response models are in `openai/chat_completions/tools/wire_models.baml`.

The common text-generation request/response wire codecs are still supplied by
the low-level `baml.llm.PrimitiveClient` host seam. Consequently there are no
placeholder OpenAI/Anthropic/Gemini wire-model files in this reference tree;
typed wire models are added only where the BAML implementation owns them.

## Layering rules

1. The standard library owns language/runtime primitives: `baml.llm`, media,
   HTTP/WebSocket transport, reflection, and provider-neutral JSON Schema.
2. `root.ai` owns portable AI contracts. `Provider` is identity plus fluent
   sugar; independent capability interfaces (`Generate`, `Tools`, `Harness`,
   and so on) state what an adapter can actually do.
3. Provider directories implement those interfaces and transform the standard
   schema only at their wire boundary. They do not leak wire models into core.
4. Scenario code owns application policy through out-of-body implementations,
   middleware, dispatchers, observers, and orchestration helpers.

Protocol data such as tool arguments and deterministic fakes uses strict
JSON decoding. SAP is reserved for unconstrained model text. Reliability
wrappers replay only providers and failures that explicitly say replay is safe;
agent and harness providers are effectful by default.
