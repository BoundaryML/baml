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
