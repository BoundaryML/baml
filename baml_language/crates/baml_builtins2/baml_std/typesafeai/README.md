# TypeSafe JEV

The wire format follows the [TypeSafe API reference](https://docs.typesafe.ai/api).

Use `client: "typesafeai/jev-latest"` in an LLM function, or construct `typesafeai.Client.new(model = "jev-latest")`. The client reads `TYPESAFE_API_KEY` at invocation time. `api_key` and `base_url` accept literal strings or late-bound `env.NAME` references. The default base URL is `https://api.typesafe.ai/v1`; requests POST to `/systemone`. `render` previews the request without reading the API key. `request_timeout_ms` and `capture_wire` follow the other built-in clients.

```baml
function IsUrgent(message: string) -> bool {
    client: "typesafeai/jev-latest"
    prompt: `${role("instructions")}Does this ticket require immediate attention?${role("user")}${message}`
}
```

JEV is a classifier. `bool` uses a Noul question and returns true at probability ≥ 0.5; `float` preserves that probability. Enums and finite unions of literals, enum variants, enums, and null use Choice. Choice decoding trusts `choice`, without consulting `probabilities`. Enum variant aliases become choice keys, descriptions become criteria, and skipped variants are excluded. A scalar uses the question ID `result`.

Classes flatten into dotted declared field paths and decode back into typed instances, including realized generic classes. Field descriptions override enum descriptions. A union does not inherit descriptions from its members. `role("instructions")` content prefixes every question and is removed from state. Nested paths and field aliases prefix the local question instruction; aliases never alter question IDs. Other prompt roles and text history are concatenated into state in order.

Unsupported outputs, recursive classes, tools, media, empty instructions, empty classes, and Choice questions outside 2–255 distinct options raise `ai.errors.InvalidRequest` before HTTP. Singleton literal fields are rejected along with their containing class. JEV does not implement the streaming client interface. Compile-time rejection is deferred.

Colliding choice keys are tolerated and belong to the first **reflected** member. The compiler currently canonicalizes union order, so swapping declaration order does not necessarily change the winner. Preserving declaration order is a compiler follow-up; clients must not rely on collisions to select a particular runtime type.

The adapter's plan and decoding are written in BAML reflection. Four small native operations expose literal values, enum values, the skip flag, and validated class construction. `ai.ModelTurn.parsed_output` carries the exact typed result through the runner, avoiding a second heuristic SAP parse that could change a union member. Its wrapper distinguishes a decoded null from an absent decoded result, and the runner checks the value against its output type.

Full response envelopes, including probabilities, confidence, and usage, remain available in `ai.events.LLMCall.http_response.body` through the usual event hooks. `capture_wire = false` omits bodies; captured authorization headers are redacted.

Tests live in `crates/baml_tests/baml_src/ns_llm_typesafeai`. From `baml_language`, run `target/debug/baml-cli test --project crates/baml_tests/baml_src -i llm_typesafeai`. For opt-in live coverage, run `infisical run --env=dev-humans -- target/debug/baml-cli test --project crates/baml_tests/baml_src --profile live -i llm_typesafeai`.
