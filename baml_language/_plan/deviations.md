# Deviations from the LLM-provider plan

A running log of where the **implementation** diverged from [`llm-provider-plan.md`](./llm-provider-plan.md)
and the design corpus in [`../llm-provider/`](../llm-provider/), and why. Updated as work proceeds.

Legend: **[lang]** forced by a missing/limited language feature · **[scope]** deliberate v1 simplification ·
**[user]** user-directed · **[design]** an intentional design refinement.

---

## Error model

- **D1 — `UnknownError.from<T>` / `with_message<T>` omitted.** **[lang]** The design's "reassert the
  channel" helpers need a runtime *"is this value a (monomorphized) `T`?"* test. BAML does not support it:
  `v is T` on a generic parameter always returns `false`, and `match (v) { let t: T => … }` is an
  irrefutable catch-all (proven in scratch). No `type_of_value` reflection exists either. **Workaround:**
  normalize foreign errors at the `catch` site with an interface-match arm —
  `catch (e) { let c: CallError => throw c, _ => throw UnknownError { … } }`. Shipped this way in
  `ns_errors/capability.baml` + `ns_ai/openai.baml`.

- **`ExtendUnknownError<E>` type alias not created.** **[lang]** Generic type aliases (plan P1) are not
  implemented. We inline `E | UnknownError` in every `throws` clause instead. Ergonomic cost only.

- **Only `CallError` implemented (not the full D8 axis).** **[scope]** Shipped `UnknownError` + `CallError`
  (3 classifiers). The shared `baml.errors.Failure` base + `is_retryable`/`is_effectful`/… axis (D8), and
  `StreamError`/`RealtimeError`/`ToolError`, are deferred until the capabilities that need them land.

## Value + sidecar

- **`(T, V)` tuple → `CallResult<T, V>` class.** **[user]** Tuples are not yet a language feature, so
  `call_with` returns a `class CallResult<T, V> { value, meta }` instead of `(T, U)`.

- **`call_with<T, V, E2>` threads an explicit callback error param `E2`.** **[lang]** The design wrote
  `call_with<T, U>(prompt, project: (ResponseMeta) -> U)`. BAML requires the callback's `throws` to be
  named and propagated (the `Iterator.map<R, E2>` pattern), so the real signature is
  `call_with<T, V, E2>(prompt, project: (ResponseMeta) -> V throws E2) -> CallResult<T, V> throws … | E2`.

## Capability spine

- **`prompt: string` (not `baml.llm.PromptAst`).** **[scope]** The initial provider took a plain `string`.
  Being revisited during the client/function wiring (see below).

- **Namespacing.** **[design]** Interfaces live in `baml.ai.*` (`ns_ai/`), SAP in `baml.sap.*` (`ns_sap/`),
  errors in `baml.errors.*` (`ns_errors/`) — rather than the design's `baml.UnknownError` (root) and the
  `ai`/`openai` package split. Keeps everything in the existing `baml` package layout.

## SAP

- **`baml.sap.parse<T>` exposed** (plan P5) — matches the plan. Public wrapper over the internal
  `__sap_parse_final<T, T>` (`ns_sap/sap.baml`). No deviation, noted for completeness.

## Wiring (client + LLM function → provider)

- **Orchestrator-level delegation, not client-as-sugar rewrite.** **[scope]** The plan's Phase 1 rewrites
  `client` into `function → Provider` and desugars the LLM function into
  `match (client()) { let h: HttpProvider => h.call<T>(…) }` inside `lower_cst.rs`. That is a deep,
  high-blast-radius change to client/function lowering (would churn every existing LLM test + snapshot).
  Instead, v1 keeps all lowering and client construction **unchanged** and delegates *inside the
  orchestrator* (`ns_llm` `execute_once_oneshot` / `call_llm_function`): for `provider == "openai"`, build a
  `baml.ai.OpenAi` and call its `HttpProvider.call<T>`; all other providers keep the legacy path. This is
  genuine end-to-end wiring through the new BAML provider for OpenAI, with the literal client-sugar rewrite
  left as a follow-up.

- **PromptAst → text bridge (`baml.llm.prompt_to_text`).** **[scope]** The orchestrator renders a
  `PromptAst`; the BAML provider takes a `string`. Rather than change the provider signature (which would
  churn the standalone tests), added a small host fn `baml.llm.prompt_to_text(PromptAst) -> string`
  (`ns_llm/llm_types.baml` + `sys_ops`) that flattens message contents to text, and convert in the
  orchestrator. **Role structure is flattened** for v1 — system/user/… roles collapse into the concatenated
  text (a single user message). Multi-message/role threading is a follow-up.

- **No `catch` in the delegation; `UnknownError` propagates.** **[lang]** `OpenAi`'s methods only ever
  `throw UnknownError` (foreign errors are normalized there), so the *inferred* throw set of `oai.call<T>`
  is `UnknownError`, not the declared `CallError | UnknownError`. A `catch (e) { … let c: CallError => … }`
  arm is therefore flagged **unreachable** (strict under `baml-cli`, only a warning under `baml_test!`). The
  delegation lets the error propagate to the existing retry loop instead of catching.

- **Model lookup uses `?? "gpt-4o"`, not a `request_body` fallback.** **[lang]** `model` is a known
  `PrimitiveClientOptions` field, so `options { model "…" }` populates `primitive.options.model`. An attempted
  `match (request_body.get("model")) { let rm: string => …, _ => … }` fallback hit a **new language finding**
  (below) and was dropped.

## Streaming

- **`Streaming` reuses the existing `baml.llm.Stream` + accumulator (leaf primitives).** **[scope]** The
  request (endpoint/headers/body incl. `api_key`/`base_url`, `"stream": true`) is built in BAML and opened
  with `baml.http.fetch_sse`; SSE-delta accumulation + finish-reason validation reuse the existing
  `StreamAccumulator` via a `from_shorthand("openai/<model>")`-built `PrimitiveClient` (both are
  provider-config leaves, not request logic). No new host fn. `from_shorthand` works without an env key.

- **`Retry` does not retry streams.** **[design]** A broken stream can't be transparently retried (partial
  output already emitted), so `Retry` forwards `.stream` once — matching the design note that streaming retry
  is connect-only.

## Value + sidecar / usage

- **`type Body = string` (not `baml.http.Response`).** **[lang]** `Response.text()` is **not idempotent**
  (reading the body consumes it), so a codec that reads the body in both `parse` and `meta_of` gets an empty
  string on the second read (usage came back `0|0`). Fixed by making the `HttpProvider.Body` associated type
  the **already-read body text** (`send` reads it once, `parse`/`meta_of` share the string). The design's
  `Body = Response` assumes re-readable bodies; ours reads once. *(Worth a compiler/runtime look: either make
  `Response.text()` idempotent for buffered bodies, or document it.)*

- **`ResponseMeta` accessors are `throws never` (best-effort).** **[design]** `finish_reason`/`usage` return a
  default on malformed wire rather than throwing, so a `call_with` projection is infallible (`E2 = never`).
  Scenarios 32/34: metering rides `call_with(prompt, m => m.usage())` returning `(value, Usage)`.

## Tools / agentic loop (scenario 09)

- **`Tool.parameters` is a JSON-Schema *string*, not `type`.** **[scope]** The design uses
  `Tool { parameters: type }` + `baml.reflect.type_to_json_schema(...)`. No `type -> JSON Schema` host fn
  exists (only `reflect.type_of`), and writing a correct one is a large Rust task — so `Tool.parameters` is an
  app-provided JSON-Schema string for now (how OpenAI tools are normally written). Generating it from a `type`
  is the clear follow-up (plan P7 / D6).

- **`run_tools` takes a `dispatch: (ToolCall[]) -> ToolResult[]` closure**, not `ExecutionContext.dispatch`.
  **[design]** Simpler and self-contained; the closure is the app's tool executor.

- **`step` mutates the transcript.** **[design]** OpenAI needs the assistant tool-call turn preserved before
  the tool-result messages; `step` appends the assistant message to the transcript's messages when it returns
  `ToolCalls`, and `submit` appends the tool results. The transcript is a mutable `messages: json[]`.

- **`function` is a reserved keyword.** **[lang]** OpenAI tool-calls have a `function` JSON key, but a BAML
  class field can't be named `function`. Tool-calls are parsed via `baml.json.field` navigation instead of a
  typed wire class.

## Language findings surfaced during wiring (worth a compiler look)

- **Matching `unknown` / `unknown?` with a typed binding is an irrefutable catch-all.** `match (v) { let x: string => …, _ => … }`
  where `v: unknown` (or `unknown?`, e.g. `map<…, unknown>.get(k)`) treats `let x: string =>` as matching
  *everything*, making the `_` arm **unreachable** (E0063). This is the same shape as the earlier
  generic-parameter finding (`v is T` / `match { let t: T => }` on a generic `T`). It means a value typed
  `unknown` cannot be runtime-type-tested by `match` — relevant to the design's `ToolCall.args: map<…, unknown>`
  coercion (D6) and any `data: unknown` handling. Workaround: avoid matching `unknown` for a type test.

