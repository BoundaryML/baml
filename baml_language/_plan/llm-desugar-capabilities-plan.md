# LLM function desugaring & the open capability registry — design + phased plan

**Status:** approved design (4 decisions settled with user 2026-07-07) · **Follows:** [`llm-provider-plan.md`](./llm-provider-plan.md) (this executes its P3/client-sugar debt + the companion desugar), [`implementation-checklist.md`](./implementation-checklist.md) (the "client-as-sugar rewrite" deferred item), [`deviations.md`](./deviations.md) (retires the "orchestrator-level delegation, not client-as-sugar rewrite" deviation).

## What this plan delivers

1. **Every LLM declarative function takes an optional `client: baml.ai.Provider` parameter** — `Extract(doc, client = Anthropic { … })` swaps the provider at the call site, combinators included.
2. **`//baml:llm_capability` — an open capability registry.** Capabilities are no longer just a convention ("interfaces that `requires Provider`"); they are *registered* declarations the compiler can enumerate — in the stdlib **and in user packages**.
3. **LLM functions truly desugar into companion calls.** `Foo(args)` negotiates `HttpProvider`; `Foo$stream`, `Foo$with`, `Foo$run_tools`, … are generated **per registered capability** — including user-defined ones. The `_openai_delegation_ok` orchestrator special-case is retired.
4. **A user can declare a custom capability + custom provider and get generated companions on their own LLM functions**, with typed runtime `Unsupported` errors when a swapped-in client lacks the capability.
5. **`ns_ai_examples` → `ns_ai_scenarios/`**, numbered like `llm-provider/ideas/scenarios/`, with runnable tests (offline fakes + a `testset "integ-test"` live tier for OpenAI/Anthropic) that exercise the *desugared* functions, not hand-written `h.call` matches.

### Settled design decisions (user-approved)

| # | Decision | Choice |
|---|---|---|
| Q1 | Type of the injected `client` param | **`baml.ai.Provider`** (existential marker). Legacy `client<llm>` configs bridge via a `LegacyClient` provider class. |
| Q2 | How capabilities register companions | **Marker + BAML driver function**: `//baml:llm_capability` on the interface, `//baml:llm_companion(<suffix>)` on a generic driver function the generated companion calls. Same mechanism for stdlib and user capabilities. |
| Q3 | Companion surface | **`$` companions** (`Foo$stream`, `Foo$run_tools`, …) — the existing compiler convention. Dot-sugar (`Foo.stream`) is a possible later parser rewrite, out of scope. |
| Q4 | Legacy retirement scope | **Desugar + bridge.** True compile-time desugaring; ported providers (OpenAI/Anthropic/Gemini) resolve to native `baml.ai` classes, unported ones (azure, bedrock, vertex, ollama, …) resolve to the `LegacyClient` bridge. Full port of the tail is out of scope. |

---

## Current state (branch vs `origin/canary`, verified 2026-07-07)

What the branch already has (see [`../llm-provider/REALIZED.md`](../llm-provider/REALIZED.md) / [`E2E_TESTS.md`](../llm-provider/E2E_TESTS.md)):

- **`baml.ai` model is real**: `Provider` marker, `HttpProvider` (messages-primitive `call_messages_with`, string sugar `call`), `Streaming`, `Tools`, `Constrained`, `Realtime`, stateful capability shapes; combinators (`Fallback`/`Retry`/`RoundRobin`); native **OpenAi / OpenAiStrict / OpenAiResponses / OpenAiRealtime / Anthropic / Gemini** providers written in BAML; mandatory `Failure` axis on all error channels; 71 green tests across the `ai_*` Rust suites incl. a live tier.
- **E0125 fixed** — user-authored providers work end to end (`ai_user_provider.rs`).
- **Wiring is runtime delegation, not desugaring**: `ns_llm/llm_types.baml` `execute_once_oneshot` checks `_openai_delegation_ok(primitive, …)` and routes `provider == "openai"` through `baml.ai.OpenAi.call_messages<T>(root.ai.prompt_to_messages(specialized))` (`llm_types.baml:365-388`, streaming at `:243-280`). Anthropic/Gemini native classes exist but the orchestrator does **not** route to them.
- **The hidden client param already exists**: `append_default_client_param` (`lower_cst.rs:611-633`) injects `client: baml.llm.Client = <declared>` into every LLM function; a user-declared `client` param is rejected (`ReservedLlmClientParam`, `lower_cst.rs:635-657`). The parser already accepts `f(client = expr)` — `parse_call_arg` special-cases `TokenKind::Client` before `=` (`parser.rs:6474`).
- **Companions already exist**: `Foo$render_prompt` / `Foo$build_request` / `Foo$build_request_stream` (`companions.rs:26-40`, name scheme `{parent}${target}`, params inherited with the client threaded), `Foo$parse` (PPIR, stream-expanded), and a working `Foo$stream`.
- **Intrinsic markers exist as CST comment scans**: `has_baml_marker` (`docstring.rs:56-79`) matches `//baml:<marker>` in leading trivia; current markers (`mut_vm`, `vm`, `fallible`, `may_yield`, `mut_self`, `tagged_string`) are consumed by builtins extraction and lowering. There is **no** interface-level marker yet, and no capability registry — capabilities are pure convention.
- **`ns_ai_examples/` is 7 files, 100% compile-only showcase** (zero `test` blocks), scenario coverage scattered across `cross_cutting/harness/misc/realtime/stateful/tools_extras/workflows.baml`.
- **Test infra**: `testset "name" { test "…" { … } }` syntax works; `baml test -i/-x "Testset::TestName"` glob-filters; the `baml_src` Rust suite runs test blocks by shelling `baml-cli test --from baml_src` (`tests/baml_src.rs:147`), plus bytecode-snapshots the whole tree.

---

## Part 1 — Design

### 1.1 The `client` parameter (Q1)

The injected parameter changes type: `client: baml.llm.Client` → **`client: baml.ai.Provider`**, still optional with the declared client as its default.

```baml
// user writes:
function Extract(doc: string) -> Resume {
  client "openai/gpt-4o"
  prompt #"Extract a resume from: {{ doc }}"#
}

// compiler sees (after injection):
function Extract(doc: string, client: baml.ai.Provider = /* resolved default */) -> Resume { … }

// call sites:
Extract(doc)                                       // declared client
Extract(doc, client = baml.ai.Anthropic { … })     // any Provider
Extract(doc, client = GPT4().fallback_to(Claude()))// combinators are Providers
Extract(doc, client = MyCustomProvider { … })      // user-authored provider
```

**Default resolution** (replaces the current `Client{…}` default expr, `lower_cst.rs:659-700` + `synthesize_client_*` at `:2393+`):

- **Shorthand `client "openai/gpt-4o"`** → an inline construction of the *native* provider class when the provider is ported: `baml.ai.OpenAi { model: "gpt-4o", api_key: env-default, … }`. A small shorthand→class map (openai → `OpenAi`, anthropic → `Anthropic`, google/gemini → `Gemini`) lives where `PROVIDER_CONFIGS` is generated today (`build.rs` / `client_fields_generated.rs`).
- **Named user client function** — `client GuardedClient` where `GuardedClient` resolves to a zero-arg (or fully-defaulted) user function returning `baml.ai.Provider` → the default expr is just `GuardedClient()`. This is plan-P3's client-as-sugar in the reference position, and it's how a *custom* provider becomes a function's **declared** client (see §1.4).
- **Named `client<llm> Foo { provider "…" options { … } }`** → for ported providers, lower the options block onto the native class fields (the same mapping `_openai_from` does at runtime today: model/api_key/base_url/headers→extra_headers/request_body→extra_body, `append_output_schema: false`). For **unported** providers (azure-openai, bedrock, vertex, ollama, …) and for configs the native class can't express (`query_params`, finish-reason lists — today's `_openai_delegation_ok` bailouts), lower to the **bridge**:

```baml
// ns_ai/core/legacy.baml (new)
class LegacyClient {
  inner: baml.llm.Client            // the untouched legacy config object
  implements Provider {}
  implements HttpProvider { … }     // delegates to the legacy orchestrator (Client.execute_oneshot)
  implements Streaming    { … }     // delegates to the legacy stream path
}
```

The bridge is the **only** consumer of the legacy Rust pipeline going forward. Two implementation notes:
- The legacy path is template-driven (jinja string + args + prompt closure), while capability drivers thread a rendered `PromptAst` (§1.3). The bridge therefore needs either (a) a `baml.llm.messages_to_prompt(ChatMessage[]) -> PromptAst` host fn (reverse of `prompt_to_messages` — lossless for role+text/media parts), or (b) drivers passing `PromptAst` so the bridge consumes it directly. **Recommended: (b), thread `PromptAst` through drivers** — native providers call `prompt_to_messages` themselves, the bridge feeds the ast straight into `PrimitiveClient.build_request(specialized, return_type)`, and nothing is lossy.
- The legacy `Client` carries retry/fallback strategy (`ClientType.Fallback/Retry` sub-clients). The bridge wraps the *whole* legacy Client, so legacy strategies keep working unchanged inside it; new-model combinators wrap outside. Document that stacking both is legal but redundant.

**Kept guardrails:** a *user-declared* `client` param on an LLM function stays rejected (`ReservedLlmClientParam`) — the param is compiler-owned. Non-LLM functions are untouched. The formatter's existing inability to format functions with a `client`-named param (gotchas) must be fixed or explicitly tolerated in Phase 2 — it will now see one in every LLM function signature it round-trips through companions.

### 1.2 The capability registry — `//baml:llm_capability` + `//baml:llm_companion` (Q2)

Two new intrinsic markers, parsed with the existing `has_baml_marker` CST scan and threaded to the AST (new fields on `InterfaceDef` / `FunctionDef`), then collected into a **capability registry** available at companion-expansion time:

```baml
// ns_ai/capabilities/streaming.baml
//baml:llm_capability
interface Streaming requires Provider {
  function stream_messages<TStream, TFinal>(self, messages: ChatMessage[]) -> baml.llm.Stream<TStream, TFinal> throws …
  …
}

//baml:llm_companion(stream)
function drive_stream<TPartial, T>(client: Provider, prompt: baml.llm.PromptAst)
    -> baml.llm.Stream<TPartial, T>
    throws baml.errors.StreamError | baml.errors.UnknownError {
  match (client) {
    let s: Streaming     => s.stream_messages<TPartial, T>(prompt_to_messages(prompt)),
    let h: HttpProvider  => _buffer_call_as_stream<TPartial, T>(h, prompt),  // honest degrade
    _ => throw baml.errors.Unsupported { message: "client's provider supports neither Streaming nor HttpProvider" },
  }
}
```

**Rules of the mechanism:**

- `//baml:llm_capability` is legal only on an `interface` that (transitively) `requires baml.ai.Provider`; anything else is a new diagnostic. It registers the interface as a capability. (The `requires` closure already exists — `interfaces.rs:165-175` `build_interface_requires_closure`.)
- `//baml:llm_companion(<suffix>)` is legal only on a free `function` whose signature matches the **driver convention**:
  - first param `client: Provider` (the existential),
  - second param `prompt: baml.llm.PromptAst`,
  - zero or more **extra params** after that (tools, dispatch closures, projections, io channels…) — copied verbatim onto the generated companion,
  - generic over **one** type param `<T>` (instantiated with the LLM function's return type) **or two** `<TPartial, T>` (first instantiated with the *stream-expanded* return type, second with the return type — the same expansion `Foo$parse` already performs in PPIR). This two-arity convention is the entire "type-arg mapping DSL" — anything fancier is rejected.
  - `<suffix>` must be a valid identifier, unique across the registry (duplicate suffix across packages = diagnostic; a user package may **not** shadow a stdlib suffix).
- The **negotiation/degrade logic lives entirely in the BAML driver** — the compiler only splices names and substitutes types. This is what makes user-defined capabilities first-class: a user writes the same two markers in their own package and their suffix appears on their LLM functions.
- Registry scope: **stdlib capabilities + capabilities declared in the compiling package** (and its deps, when packages-as-deps exist). Companion generation for a function sees the union. Stdlib suffixes generate from Phase C; **user-package suffixes generate in Phase D** — the mechanism is identical, the later phase just gates the cross-package pre-pass.
- **Why generate companions at all (vs "just call your provider directly")?** The generated `Foo$moderated(args…, policy, client?)` body is exactly the one-liner `drive_moderated<T>(client, Foo$render_prompt(args…), policy)` — which a user can always write by hand, so nothing *functional* is gated on user-companion generation. What generation buys: (a) the function's arg→prompt threading without restating it per call site (unwritable generically in userland — BAML can't abstract over "any LLM function's params"); (b) `T` substituted from the function's declared return type; (c) the declared client as the param default; (d) **stream-expanded partial types for two-arity drivers — the one thing a user genuinely cannot spell by hand**; (e) a uniform, discoverable `Foo$x` surface where swapping the interaction mode is symmetric with swapping the client.

**Stdlib registrations shipped by this plan** (drivers live next to their capability file):

| Suffix | Capability | Companion signature (generated, after user params) | Driver degrade chain |
|---|---|---|---|
| *(none — the function itself)* | `HttpProvider` | `Foo(args…, client?) -> T` | `HttpProvider.call_messages<T>` → drain a `Streaming` provider → `Unsupported` |
| `with` | `HttpProvider` | `Foo$with(args…, project: (ResponseMeta) -> V throws E2, client?) -> CallResult<T, V>` | `call_messages_with<T,V,E2>` → `Unsupported` |
| `stream` | `Streaming` | `Foo$stream(args…, client?) -> baml.llm.Stream<Partial, T>` | `stream_messages` → buffer an `HttpProvider.call` → `Unsupported` |
| `run_tools` | `Tools` | `Foo$run_tools(args…, tools: Tool[], dispatch: (ToolCall[]) -> ToolResult[], client?) -> T` | `Tools.run_tools` → `Unsupported` (no honest degrade) |
| `live` | `Realtime` | `Foo$live(args…, io: Channel, client?) -> void` | `Realtime.run` → `Unsupported` (no honest degrade) |

**The main function is the primary surface — tools ride the client, not the call site.** The original design (scenarios [09](../llm-provider/ideas/scenarios/09-tool-calling/usage.baml)/[10](../llm-provider/ideas/scenarios/10-agentic-loop/usage.baml)) already put the **loop policy** in the client: `Bounded { inner, stop_when, per_turn_tools }` is a Provider+Tools combinator slotting into `client:`, with a predicate vocabulary (`step_count_is(8)`, `has_tool_call("final_answer")`, `any_of([…])`) and `run_to_budget<T> -> T | Budget<T>` for budget partials (plan-D5). But it kept **tools + dispatch at the call site** (`Research.run_tools(question, tools_for_research(), ctx)`), so the plain call never ran tools. This plan closes that last step (user direction): the combinator **also carries the tools and dispatch, and implements `HttpProvider`** by driving the loop over its `Tools`-capable inner — so the agentic loop is implied by the provider and plain `Foo(args)` returns `T`:

```baml
client SmartResearcher() {
  baml.ai.ToolLoop {                     // scenario 10's Bounded + tools/dispatch on board, implements HttpProvider
    inner: baml.ai.OpenAi { … },         // must be Tools-capable at runtime, else typed Unsupported
    tools: tools_for_research(),          // Tool.from_type(…) — typed params, schema lowered from `type`
    dispatch: dispatch_research,          // (ToolCall[]) -> ToolResult[]
    stop_when: any_of([ step_count_is(8), has_tool_call("final_answer") ]),
    per_turn_tools: null,                 // optional (turn, history) -> Tool[] filter
  }
}

Research(question)                        // client: SmartResearcher() — the loop is implied; returns T
Research(question, client = SmartResearcherGemini())   // swap the whole stack at the call site
```

Per scenario 10's own caveat (retry around a tool loop re-drives every side-effecting dispatch), `ToolLoop` sets the plan-D2 effect marker (`is_effectful() -> true`) so `Retry`/`Fallback` refuse to wrap it with a typed `CannotRetry` instead of silently replaying tools.

`Foo$run_tools(args…, tools, dispatch, client?)` stays generated as the original design's **explicit-control form** — per-call tools, approval gates around `begin`/`step`/`submit` (09's `ApprovedSupport`), deps injection, and reaching `run_to_budget` for the `T | Budget<T>` outcome. The same pattern applies to any capability that composes as a wrapper: prefer configuring the client over adding call-site modes; companions exist only where the *invocation shape itself* differs (`$stream` returns a `Stream`, `$live` needs an `io: Channel`, `$with` returns value+meta).

(The degrade chains are exactly `_conventions.md` §6: call↔stream degrade both ways; `run_tools`/`live` error when absent. Non-companion capabilities — `Chain`, `Background`, `Conversational`, `ManagedCache`, `Suspendable`, introspection — stay method-surface-only: they are stateful/handle-shaped, and per plan-D2 a generated per-function companion would be dishonest. They keep `//baml:llm_capability` for registry visibility but simply have no `//baml:llm_companion` driver.)

**Phase-ordering note (the one real compiler-shape risk):** companions are expanded during AST lowering (`companions.rs`), per file — but the registry spans the package. Add a cheap **pre-pass**: scan every file's CST for the two markers (comment-trivia scan, no type resolution needed — suffix, driver name, driver param list, driver generic arity, declared throws) and hand the resulting table to companion expansion. Stdlib capability tables can be baked at `build.rs` time exactly like `PROVIDER_CONFIGS`. Full validation (marker on a real interface, driver signature conformance, `requires Provider` closure) happens later in HIR/TIR where types exist, as diagnostics — generation is by the syntactic table, checking is semantic.

### 1.3 LLM function desugaring (Q3, Q4)

The LLM function body stops lowering to `baml.llm.call_llm_function<T>(client, name, args_map, prompt_closure)` (`lower_expr_body.rs:296-394`) and instead lowers to the same shape every companion has:

```baml
// Foo(args…, client = <default>) -> T   — the generated body:
{
  let prompt: baml.llm.PromptAst = Foo$render_prompt(args…, client = client);
  drive_call<T>(client, prompt)
}

// Foo$stream(args…, client = <default>) — generated:
{
  let prompt: baml.llm.PromptAst = Foo$render_prompt(args…, client = client);
  drive_stream<Partial<T>, T>(client, prompt)
}

// Foo$run_tools(args…, tools, dispatch, client = <default>) — generated:
{
  let prompt: baml.llm.PromptAst = Foo$render_prompt(args…, client = client);
  drive_run_tools<T>(client, prompt, tools, dispatch)
}
```

- **`Foo$render_prompt` is reused as-is** (it already exists and already threads the client for provider-specific specialization; backtick prompts already pre-lower a `(Context) -> PromptAst` closure, jinja renders via the template string). Schema injection keeps today's semantics: the rendered template carries `ctx.output_format` iff referenced; native providers are constructed with `append_output_schema: false` on this path (the `e2e_structured_schema_injected_once` behavior).
- **Prompt rendering against a swapped client:** `render_prompt`'s specialization is provider-keyed. With `client` now a `Provider`, specialization becomes capability-shaped: a `PromptSpecializer` hook on the provider (default = identity), with `LegacyClient` delegating to the old `specialize_prompt`. Small, but don't lose it — Anthropic system-hoisting depends on request-side handling in the native class instead (already true: `anthropic_request_shape_via_mock`).
- **Throws:** the companion's declared channel = the driver's declared channel (substituted). Drivers declare the honest per-capability channel (`CallError | UnknownError`, etc.). The strict throws-checker (E0097, exact-throws) is satisfied because the body is exactly one driver call. LLM functions today have an implicit channel — desugaring makes it explicit; the existing corpus must stay green, so companions' throws must not be *narrower* than what the legacy path could throw.
- **Retirement:** `_openai_delegation_ok` / `_openai_from` and the oneshot/stream special-cases in `execute_once_oneshot`/`execute_once_stream` are deleted. `call_llm_function`/`stream_llm_function` remain only as the internal surface `LegacyClient` delegates to (and can shrink to that role). `Foo$build_request` / `Foo$build_request_stream` / `Foo$parse` companions stay (tooling/tests use them); they keep meaning "the legacy wire shape" until the bridge itself is retired someday.
- **Proactive generation, runtime negotiation:** every LLM function gets a companion **per registered driver-bearing capability**, whether or not the declared client supports it — because the client can be swapped at the call site, support is a runtime property. A swapped client lacking the capability throws typed `baml.errors.Unsupported` from the driver's `_` arm. Code-size pressure from N companions × M functions is real; mitigation is the standard dead-code elimination story (only referenced companions survive to bytecode — verify, and if untrue today, make companion emission reference-driven at MIR while keeping *typechecking* proactive).

### 1.4 User-defined custom capabilities — the e2e story (ask #4)

The acceptance scenario that must work, entirely in user code:

```baml
// user package: capability + provider + LLM function
//baml:llm_capability
interface Moderated requires baml.ai.Provider {
  function call_moderated<T>(self, messages: baml.ai.ChatMessage[], policy: string) -> T throws …
}

//baml:llm_companion(moderated)
function drive_moderated<T>(client: baml.ai.Provider, prompt: baml.llm.PromptAst, policy: string) -> T throws … {
  match (client) {
    let m: Moderated => m.call_moderated<T>(baml.ai.prompt_to_messages(prompt), policy),
    _ => throw baml.errors.Unsupported { message: "provider is not Moderated" },
  }
}

class GuardedOpenAi {
  inner: baml.ai.OpenAi
  implements baml.ai.Provider {}
  implements Moderated {
    function call_moderated<T>(self, messages: baml.ai.ChatMessage[], policy: string) -> T throws … {
      self.inner.call_messages<T>(redact(messages, policy))   // pre/post-filter around the inner provider
    }
  }
}

function Summarize(doc: string) -> string {
  client "openai/gpt-4o"            // declared client is a PLAIN provider — deliberately NOT Moderated
  prompt #"Summarize: {{ doc }}"#
}

test "capability supplied by swapping the client in" {
  let guarded = GuardedOpenAi { inner: baml.ai.OpenAi { model: "gpt-4o", api_key: k, base_url: null } };
  let s = Summarize$moderated("Bob's SSN is 000-00-0000. He likes turtles.", "no-pii", client = guarded);
  assert.is_true(s.length() > 0)
}

test "declared client lacks the capability -> typed Unsupported" {
  let s = Summarize$moderated("some text", "no-pii") catch (err) {   // no override: plain OpenAi is not Moderated
    let u: baml.errors.Unsupported => return assert.is_true(true),
    _ => return assert.is_true(false),
  };
  assert.is_true(false)
}
```

The two tests are the point of the pair: the *declared* client doesn't have the capability, so the companion works exactly when a capable client is swapped in — and fails **typed** when it isn't.

**Declaring a custom provider as the function's default client.** The above only reaches `GuardedOpenAi` via call-site override, because `client "shorthand"` / `client<llm>` blocks can only name built-in providers. To let the *declared* client be a user provider, the `client` field also accepts a reference to a **user function returning `baml.ai.Provider`** (this is exactly plan-P3's "client is sugar for `function → Provider`", scoped to the reference position):

```baml
function GuardedClient() -> baml.ai.Provider {
  GuardedOpenAi { inner: baml.ai.OpenAi { model: "gpt-4o", api_key: baml.env.get_or_panic("OPENAI_API_KEY"), base_url: null } }
}

function SummarizeGuarded(doc: string) -> string {
  client GuardedClient              // default client IS the custom provider; override still possible
  prompt #"Summarize: {{ doc }}"#
}
```

Lowering: if the name after `client` resolves to a `client<llm>` block → today's path (native class or bridge, §1.1); if it resolves to a zero-arg (or fully-defaulted) function returning `Provider` → the injected param's default expr is simply `GuardedClient()`.

This exercises: marker parsing in a user package, registry union, companion generation against a user driver, cross-package `requires` (the fixed E0125), the runtime `Unsupported` path, and the user-function-as-declared-client lowering.

### 1.5 Scenario examples reorg (`ns_ai_examples` → `ns_ai_scenarios`)

**Layout** (in `crates/baml_tests/baml_src/`):

```
ns_ai_scenarios/
  common/
    fakes.baml            # offline providers: EchoProvider-style fake, ScriptedToolProvider, FailingProvider…
    helpers.baml          # shared assertion/setup helpers (only if genuinely shared)
  10_agentic_loop/
    usage.baml            # THE file: LLM fns + desugared-companion usage + test blocks (offline + integ-test)
  14_multi_agent/
    usage.baml
  16_agent_security/
    usage.baml
    implementation.baml   # only when the scenario defines a custom provider/capability/combinator
  …
```

- **Numbering mirrors `llm-provider/ideas/scenarios/`** (`10_agentic_loop` ↔ `10-agentic-loop`; underscores because dir/namespace idents). **The goal is the full corpus**: a `ns_ai_scenarios/NN_*` directory for every original scenario, each with runnable tests — this is a build-out, not just a move of the 7 existing files. Roughly 40 of the 47 can run end-to-end today; the P8-blocked tail (harness subprocess 37–42, durable/local stores behind 18/19/21/44–47, realtime beyond the working text exchange 23–25) gets what's achievable now (offline negotiation/shape tests against fakes, `Unsupported`-path tests) plus a `// BLOCKED: P8 <what>` marker so the gap is visible, and graduates to live tests as the host surface lands.
- Every file starts with a **relative-URI header** back to the design doc, e.g. in `ns_ai_scenarios/10_agentic_loop/usage.baml`:
  `// scenario 10 — agentic loop: ../../../../../llm-provider/ideas/scenarios/10-agentic-loop/README.md`
- **The arbitrary showcase code is culled.** Anything that neither (a) runs as a test nor (b) implements a real custom provider/capability gets deleted or rewritten as a runnable test. `usage.baml` is usage that *executes*; `implementation.baml` is the custom-provider/capability work it needs. No third kind of file.
- **Multiple tests per scenario are expected, not exceptional.** A scenario's `usage.baml` should test each distinct behavior/setting it demonstrates — happy path, the negotiation-failure path, client-swap variants, per-provider settings (OpenAI vs Anthropic), streaming vs oneshot, etc. — as separate `test` blocks (offline where a fake suffices, `integ-test` where the point is the real API). One-test-per-scenario is a smell that the scenario is under-exercised.
- **Capabilities currently squatting in the examples get triaged**: genuinely stdlib-worthy interfaces (candidates from the audit: `Stt`/`Tts` (25), `Scorer` (33), `Deterministic` (33), the `ToolKind` taxonomy (12)) move under `ns_ai/capabilities/` with the same scenario-URI headers; one-scenario contrivances stay in that scenario's `implementation.baml`.
- **Scenario back-references land in the existing stdlib capability files too** — e.g. `ns_ai/capabilities/streaming.baml` gets `// scenarios: 04-streaming — ../../../../../../llm-provider/ideas/scenarios/04-streaming/` (use `REALIZED.md`'s table as the source of the mapping).
- **Namespace check at implementation time:** confirm whether nested dirs under `ns_ai_scenarios/` share one namespace (per `baml_src.rs::namespace_key` they likely do) — if so, scenario decls need unique names (`s10_bounded_agent` prefixing or per-scenario `ns_…` dirs as fallback).

### 1.6 Test strategy — `testset "integ-test"` + desugared-function coverage

**Principle (user preference): tests are native BAML `test` blocks wherever the harness allows.** Rust tests are reserved for what BAML genuinely cannot host:
- **wiremock mock servers + request-capture** (wire-fidelity byte assertions — a BAML test can't spin up the mock endpoint),
- **compiler-phase assertions** (diagnostics, snapshots, marker-validation error codes, bytecode shape),
- **harness plumbing** (the runners that invoke `baml-cli test` with the right filters).

Everything else — negotiation behavior, combinator semantics against in-BAML fake providers, error normalization/triage, capability-model checks, and all live/API coverage — lives in `.baml` test blocks. Existing `ai_*.rs` tests that are just `baml_test!(...)` around network-free BAML source are migration candidates (Phase F); new coverage must not add Rust tests for anything expressible as a BAML test.

Two tiers inside each scenario's `usage.baml`:

```baml
// offline tier — deterministic, no network, runs in the default baml_src suite:
test "negotiation errors are typed" {
  let e = ExtractTicket(doc, client = common.NoCapProvider {}) catch (err) {
    let u: baml.errors.Unsupported => return assert.is_true(true),
    _ => return assert.is_true(false),
  };
  assert.is_true(false)
}

// live tier — real APIs, excluded by default:
testset "integ-test" {
  test "10: bounded agent loop live (openai)" {
    let agent = baml.ai.ToolLoop { inner: baml.ai.OpenAi { … }, tools: tools, dispatch: dispatch, stop_when: step_count_is(8) };
    let r = RunAgent(task, client = agent);                       // plain call; the loop is implied by the provider
    assert.is_true(r.length() > 0)
  }
  test "10: same loop live (anthropic)" {
    let agent = baml.ai.ToolLoop {
      inner: baml.ai.Anthropic { model: "claude-haiku-4-5-20251001", api_key: baml.env.get_or_panic("ANTHROPIC_API_KEY") },
      tools: tools, dispatch: dispatch, stop_when: step_count_is(8),
    };
    let r = RunAgent(task, client = agent);
    assert.is_true(r.length() > 0)
  }
}
```

- **Tests call the main function.** The default shape is plain `Foo(args)` / `Foo(args, client = …)` — including tool scenarios, which wrap the client in `ToolLoop { inner, tools, dispatch }` rather than calling `Foo$run_tools` (§1.2: the agentic loop is implied by the provider). Companions appear only when the test is *about* that invocation shape (`$stream` partials, `$with` metadata projection, `$live` channels, `$run_tools` explicit budget control). Hand-rolled `match (p) { let h: HttpProvider => h.call… }` survives only where it *is* the lesson (negotiation internals, driver-style code in `implementation.baml`).
- **Offline tier** uses `common/fakes.baml` providers (pure BAML, no network — the `EchoProvider` pattern from `ai_user_provider.rs`, moved into BAML source). This keeps real runnable coverage in the default suite.
- **Integ tier** targets **OpenAI + Anthropic** (per user direction; Gemini live stays in the Rust `ai_gemini.rs` suite for now).
- **Filtering / runners** (`tests/baml_src.rs`):
  - existing `baml_test()` gains `-x "integ-test::"` — default suite never touches the network;
  - new `#[test] fn baml_integ_test()` — env-gated like the `ai_*` live tests (skip unless `OPENAI_API_KEY`/`ANTHROPIC_API_KEY`), runs `baml-cli test --from baml_src -i "integ-test::"`. So in Rust/CI: one cargo test runs one set, the other runs the other, exactly like today's mock-vs-live split.
- The `ai_*` Rust suites **shrink to what only Rust can do**: wiremock/request-capture wire-fidelity tests stay; their network-free `baml_test!` tests (negotiation, combinators, error triage, capability checks) migrate into BAML test blocks under `ns_ai_scenarios/` or a `ns_ai_tests/` dir; their live tests migrate into the `integ-test` testset. The BAML testset layer is the primary e2e surface.

---

## Part 2 — Phases

Ordering rationale: the registry (A) is pure addition; the client param (B) changes signatures but keeps runtime behavior; the desugar (C) is the blast-radius step and lands on top of A+B; D proves the open-registry thesis; E/F ride on C so tests exercise the real surface. E's mechanical file moves can start any time, but its test-rewrite pass waits for C.

### Phase A — Capability registry
- Parse `//baml:llm_capability` (interfaces) and `//baml:llm_companion(<suffix>)` (functions) via `has_baml_marker`; new AST fields; package-wide pre-pass building the syntactic registry table; stdlib table baked at build time (alongside `PROVIDER_CONFIGS`).
- Semantic validation diagnostics (new error codes): marker on non-interface / interface not requiring `Provider`; driver signature nonconforming (first two params, generic arity 1/2, suffix ident, duplicate suffix, stdlib-suffix shadowing).
- Mark the stdlib capabilities; write the five drivers (`drive_call`, `drive_with`, `drive_stream`, `drive_run_tools`, `drive_live`) with the §1.3 degrade chains, strict-checked via `baml-cli run --file` (gotchas: E0097 exact-throws, unreachable-catch narrowing).
- Add the **`ToolLoop` combinator** (§1.2 — scenario 10's `Bounded` extended to carry `tools`/`dispatch` and `implements HttpProvider` by running `begin`/`step`/`submit` over a `Tools`-capable `inner`; pure BAML, sibling of `Fallback`/`Retry`), plus the stop-policy vocabulary from the original design (`step_count_is`, `has_tool_call`, `any_of`, `per_turn_tools`, `run_to_budget<T> -> T | Budget<T>`), so plain `Foo(args)` with a `ToolLoop` client is the primary tool surface.
- **Exit:** registry table snapshot test; drivers compile strictly; no behavior change anywhere else.

### Phase B — `client: baml.ai.Provider` + `LegacyClient` bridge
- `LegacyClient` class implementing `Provider`/`HttpProvider`/`Streaming` over the legacy `Client` (PromptAst-threading per §1.1); shorthand→native-class map; named-client lowering to native class fields (ported + expressible) or bridge (everything else).
- Retype `append_default_client_param`; keep `ReservedLlmClientParam`; fix (or consciously accept) the formatter's `client`-param limitation.
- `client <name>` accepts a user function returning `baml.ai.Provider` (§1.1/§1.4) alongside `client<llm>` references and shorthands.
- **Exit:** every existing LLM test green with the retyped param (still routed through the *current* orchestrator — no desugar yet); `Extract(doc, client = baml.ai.OpenAi { … })` works end to end as an override; a user provider works as a **declared** client via a client function; snapshot regen (`compiles/__baml_std__` + baml_src bytecode).

### Phase C — The desugar (blast-radius step)
- LLM body → `drive_call<T>(client, Foo$render_prompt(…))`; generate `Foo$<suffix>` per registered **stdlib** driver; unify `Foo$stream` onto the mechanism; prompt-specialization hook for the bridge.
- Delete `_openai_delegation_ok`/`_openai_from` and the orchestrator special-cases; `call_llm_function`/`stream_llm_function` shrink to the bridge's internals.
- Back-compat gate: the **entire existing LLM corpus** (legacy tests + `ai_*` suites + baml_src) passes; wire-fidelity request-capture tests (`e2e_structured_schema_injected_once`, `e2e_roles_preserved_on_wire`, `e2e_options_and_headers_forwarded`, media) prove parity byte-for-byte where they already assert it.
- **Exit:** `Foo`, `Foo$stream`, `Foo$with`, `Foo$run_tools` all work with declared, swapped, combinator, and bridge clients; anthropic/gemini LLM functions now route natively (they get real provider classes at lowering, no orchestrator special-case needed).

### Phase D — User-defined capabilities (registry opens to user packages)
- Enable companion generation from **user-package** `//baml:llm_companion` markers (the §1.2 pre-pass over the compiling package's files; mechanism unchanged from C). Until this lands, user capabilities are still fully usable via the direct driver call (`drive_moderated<T>(client, Foo$render_prompt(…), policy)`) — generation is ergonomics (+ stream-expanded types), not functionality.
- The §1.4 scenario as a real test project — **BAML test blocks** in a user-package fixture (capability + driver + provider + LLM function + generated companion + typed `Unsupported` + user-function-as-declared-client), plus LSP/diagnostic snapshots for the new error codes (Rust, unavoidably — compiler-phase assertions).
- **Exit:** the custom-capability suite runs as native BAML tests (only the diagnostic snapshots are Rust); docs snippet in `ns_ai` README-comment showing the two-marker recipe.

### Phase E — Scenario reorg
- Move/renumber `ns_ai_examples/*` → `ns_ai_scenarios/NN_name/{usage,implementation}.baml`; cull showcase-only code; triage squatting capabilities into `ns_ai/capabilities/` (with scenario-URI headers) vs per-scenario `implementation.baml`; add scenario URIs to existing capability files; `common/fakes.baml`.
- Rewrite surviving example code onto the desugared surface (drop hand-written `h.call` matches except where the match *is* the lesson).
- **Exit:** `ns_ai_examples/` gone; every scenario file has a scenario-URI header; baml_src suite green; snapshot regen.

### Phase F — Integ testset + Rust→BAML test migration
- Offline-tier tests per scenario against `common` fakes; `testset "integ-test"` live tests (OpenAI + Anthropic) leveraging desugared functions; `baml_test()` gains `-x "integ-test::"`; new env-gated `baml_integ_test()` with `-i "integ-test::"`.
- **Migrate the `ai_*` Rust suites toward BAML-native** (§1.6 principle): network-free `baml_test!` tests → BAML test blocks; Rust `*_live_*` tests → the `integ-test` testset; keep only wiremock/request-capture + compiler-phase tests in Rust. Delete migrated Rust tests rather than duplicating coverage.
- Update `E2E_TESTS.md` (the BAML-level integ tier becomes part of the verified surface) and `implementation-checklist.md`.
- **Exit:** default `cargo test -p baml_tests` runs no network; keyed run executes the integ set green against both APIs; remaining Rust tests in `ai_*.rs` are only those needing wiremock or compiler internals.

---

## Part 3 — Risks & open items

- **Companion/registry phase ordering** (§1.2 note) — the syntactic-table pre-pass is the load-bearing trick; if AST-lowering isolation makes even that awkward, fallback is moving companion *expansion* to the HIR item-tree step (bigger refactor; avoid).
- **Stream-expansion in the driver convention** — the `<TPartial, T>` two-arity rule must reuse the exact PPIR stream-expansion `Foo$parse` uses; divergence would give `$stream` a different partial type than `$parse`. Single source of truth required.
- **Throws-channel parity** — desugared companions surface *typed* channels where legacy surfaced UnknownError-ish behavior; the strict checker may flag previously-silent call sites in user code. Mitigate: companions declare `<capability channel> | UnknownError` (never narrower than legacy).
- **Snapshot churn** — Phase B and C each regenerate `__baml_std__` phase snapshots + baml_src bytecode + LSP listing snaps; mechanical but large; keep them in dedicated commits (the gotchas file's regen instructions apply).
- **Code size from proactive companions** — verify DCE drops unreferenced companions from emitted bytecode; if not, generate reference-driven at MIR while typechecking proactively.
- **Bridge fidelity tail** — `query_params` + finish-reason allow/deny lists route through the bridge even for `provider "openai"` (same as today's delegation bailout); acceptable, documented.
- **Formatter + `client` param** (pre-existing gotcha) — must not block Phase B; fix opportunistically.
- **Namespace layout of `ns_ai_scenarios/`** (§1.5) — verify nested-dir namespace behavior before mass-moving; fall back to per-scenario `ns_` dirs if needed.
- **Non-goals:** dot-form companion sugar (`Foo.stream`); porting azure/bedrock/vertex/ollama to native BAML (stays behind the bridge; needs the SigV4/OAuth host fns per plan P8); compile-time capability demands (plan D3 — companions keep the existential `Provider`, forward-compatible with future intersection types); generating companions for stateful/handle capabilities (dishonest per D2).
