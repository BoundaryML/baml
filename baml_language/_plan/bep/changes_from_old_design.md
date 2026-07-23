# Changes from the Design Implemented on This Branch

This document is a migration note for implementers. It is not part of the conceptual learning path in BEP-063.

The comparison was checked against the branch on 2026-07-09, primarily:

- `crates/baml_builtins2/baml_std/baml/ns_ai/`;
- `crates/baml_builtins2/baml_std/baml/ns_llm/`;
- `crates/baml_compiler2_ast/src/companions.rs`;
- `crates/baml_compiler2_hir/src/capability_registry.rs`;
- `crates/baml_compiler2_ppir/src/lib.rs`;
- `crates/baml_tests/baml_src/ns_ai_custom_capability/`;
- `crates/baml_tests/baml_src/ns_ai_scenarios/`.

## Executive summary

The branch proves several important ideas that BEP-063 keeps:

- providers and capability implementations can be ordinary BAML classes/interfaces;
- `Provider` can be an existential dynamic root;
- prompt roles and media can cross one `PromptAst -> ChatMessage[]` leaf conversion;
- built-in provider request/response logic can be written natively in BAML;
- LLM functions can lower through capability drivers;
- user classes can implement stdlib capability interfaces;
- runtime interface matching can negotiate provider support;
- typed streams, tools, response metadata, and stateful scenarios are expressible.

BEP-063 changes the extension seam. The branch generates a companion per registered capability driver. The BEP instead generates one first-class `LlmRequest<T>` per LLM invocation and makes capability drivers ordinary functions over that request.

## Surface comparison

| Branch implementation                                                                                  | BEP-063 direction                                                                                             | Reason                                                                                               |
| ------------------------------------------------------------------------------------------------------ | ------------------------------------------------------------------------------------------------------------- | ---------------------------------------------------------------------------------------------------- |
| `Provider` has `is_effectful`, `with_retry`, `fallback_to`, `traced` default methods                   | `Provider` is a marker; composition uses `baml.ai.retry`, `fallback`, `traced` functions                      | Keep the existential root small and avoid provider-wide effect classification                        |
| `HttpProvider` owns `build_request`, `send`, `parse`, `parse_meta`, `call_messages_with`, and `call`   | `Generate` owns one semantic `generate<T>(LlmRequest<T>) -> LlmResponse<T>` method                            | Transport stages are provider implementation details and are meaningless on wrappers/local providers |
| Provider primitive receives `ChatMessage[]`                                                            | Provider primitive receives `LlmRequest<T>` and may call `.messages()`                                        | Preserve prompt plus output type, identity, arguments, options, tags, and provider binding           |
| `call_with` accepts a metadata projection callback                                                     | `Generate` always returns `LlmResponse<T>`; `run` drops metadata                                              | Keep local projection outside retry/fallback I/O scopes                                              |
| `ResponseMeta` is an interface with fixed accessor methods                                             | `ResponseMeta` is normalized common data plus provider-specific attributes/raw                                | Easier serialization and extension; open question remains for typed provider sidecars                |
| `//baml:llm_capability` registers an interface                                                         | Any `interface ... requires Provider` is a capability by convention                                           | Ordinary language types already supply the contract                                                  |
| `//baml:llm_companion(suffix)` registers a driver                                                      | Driver is an ordinary generic function accepting `LlmRequest<T>`                                              | No compiler registry, suffix, or global scan is needed                                               |
| Every LLM function grows `Foo$suffix` for every registered user driver                                 | Every LLM function grows one `Foo$request`; custom libraries call their own drivers                           | Bounds generated API/code growth and removes suffix conflicts                                        |
| Standard companions include `$with`, `$run_tools`, `$live`, and `$stream`                              | Keep a small fixed family, especially `$stream`; other modes consume `$request` explicitly                    | Streaming needs compiler-derived partial types; uncommon lifecycles do not need bespoke companions   |
| Main LLM function calls `drive_call(client, Foo$render_prompt(...))`                                   | Main function calls `run(Foo$request(...))`                                                                   | Put all semantic invocation context in one value                                                     |
| `prompt` tagged template returns lazy `(Context) -> PromptAst`                                         | Preserved                                                                                                     | This is the correct rendering model                                                                  |
| `prompt_to_messages(PromptAst)` is the native leaf bridge                                              | Preserved behind `LlmRequest.messages()`                                                                      | Roles/media remain structural; provider authors get a convenient semantic request API                |
| `Streaming.stream_messages<TPartial,T>(messages)`                                                      | `Streaming.stream<TPartial,T>(LlmRequest<T>)`                                                                 | Streaming also needs identity, options, schema, and trace context                                    |
| `Background.submit<T>(prompt: string, idempotency_key) -> Job<T>`                                      | `Background.submit<T>(LlmRequest<T>, options) -> Job<T>`                                                      | Do not flatten/rewrite an LLM function prompt; keep typed task context                               |
| `Job<T> { id, owner }`; provider has `poll(job) -> T?`                                                 | `Job<T>` is a resource interface with `poll`, `status`, `cancel`, `token`, `cleanup`; provider resumes tokens | Resource owns its identifier, parser, lifecycle, and provider                                        |
| `Session`, `ChainHandle`, `Window`, and `CacheHandle` are data handles passed back to provider methods | Provider returns `Session`, `Job<T>`, `CacheResource`, etc. with owned behavior                               | Prevent wrong-provider calls and make lifecycle discoverable                                         |
| `Realtime.run(prompt: string, io) -> Transcript` plus provider-level `LiveControl`                     | `Realtime.open_live(LlmRequest<T>, channel) -> LiveSession`; control methods live on the resource             | Cancellation/truncation target one live session, not an arbitrary channel                            |
| Provider-wide `is_effectful() -> bool` gates combinator re-drive                                       | Per-operation `ReplayPolicy` plus per-error `CommitState`                                                     | One provider may have safe reads, idempotent submits, and never-replay live operations               |
| `Capabilities` reports a fixed set of graded features                                                  | Interfaces report operation existence; optional descriptors report graded/payload support                     | Separate callable contracts from model/payload limits                                                |
| Retry/fallback classes implement the full `HttpProvider` codec and throw from meaningless stages       | Capability-specific wrappers implement only semantic operations they can forward                              | No fake codec methods on combinators                                                                 |

## What remains unchanged

### Providers remain BAML values

Built-in and user providers remain ordinary classes:

```baml
class MyProvider {
  implements baml.ai.Provider {}
  implements baml.ai.Generate { ... }
}
```

The BEP does not return to a closed Rust provider enum.

### Capability negotiation remains interface dispatch

```baml
match (request.provider) {
  let provider: MyCapability => ...,
  _ => throw baml.errors.Unsupported { ... },
}
```

The difference is that the driver is ordinary code rather than compiler-registered companion machinery.

### The prompt stays structural

The branch's native `ChatMessage`/`MessagePart` representation and single `prompt_to_messages` host bridge are retained. BEP-063 wraps them in a richer semantic request; it does not flatten prompts to strings.

### Provider wire logic stays in BAML

OpenAI, Anthropic, Gemini, OpenAI-compatible, Responses, and realtime implementations can continue to build wire requests and decode wire responses in stdlib BAML. Narrow host helpers remain appropriate for cryptography, transports, and opaque runtime state.

### LLM functions remain the user-facing task abstraction

The main call and `$stream` remain concise. The BEP adds `$request` so advanced lifecycles reuse rather than bypass the LLM function.

## Current generated companion design

The branch uses two marker comments:

```baml
//baml:llm_capability
interface Moderated requires baml.ai.Provider {
  function call_moderated<T>(
    self,
    messages: baml.ai.ChatMessage[],
    policy: string,
  ) -> T
}

//baml:llm_companion(moderated)
function drive_moderated<T>(
  client: baml.ai.Provider,
  prompt: baml.llm.PromptAst,
  policy: string,
) -> T {
  ...
}
```

The compiler registry validates the driver shape, reserves `moderated`, and synthesizes `Foo$moderated` for LLM functions. It substitutes `T` with the final return type, optionally substitutes `TPartial` with the stream-expanded type, copies other generics, copies extra parameters, and root-absolutizes user-package types.

That machinery works, but creates these long-term costs:

- companion suffixes are session-global names;
- the compiler must understand driver generic conventions;
- every installed driver affects every LLM function's semantic item set;
- signatures copied across packages need path rewriting;
- capability discovery and companion generation depend on whole-package registry queries;
- errors in one driver can affect unrelated LLM functions;
- host SDKs must decide whether to expose arbitrary generated companions;
- the number of generated functions scales with functions times drivers.

## Replacement extension path

The branch example:

```baml
ComposeNote$moderated(
  "turtles",
  "no-pii",
  client = GuardedEcho { reply: "a draft note" },
)
```

becomes:

```baml
run_moderated(
  ComposeNote$request(
    "turtles",
    client = GuardedEcho { reply: "a draft note" },
  ),
  "no-pii",
)
```

The user still writes:

- one capability interface;
- one provider implementation;
- one driver.

They no longer write marker comments or depend on compiler synthesis.

## `HttpProvider` migration

### Current primitive

```baml
interface HttpProvider requires Provider {
  type Body
  function build_request<T>(self, messages: ChatMessage[]) -> baml.http.Request
  function send(self, request: baml.http.Request) -> Body
  function parse<T>(self, from: Body) -> T
  function parse_meta(self, from: Body) -> ResponseMeta
  function call_messages_with<T,V,E2>(...) -> CallResult<T,V>
}
```

### Proposed primitive

```baml
interface Generate requires Provider {
  function generate<T>(self, request: LlmRequest<T>) -> LlmResponse<T>
}
```

Leaf HTTP providers move their existing default-method sequence into `generate`. Shared helpers can retain an internal `HttpCodec` protocol. Combinators stop implementing codec stages and implement `Generate` by delegating to member `Generate` values.

The provider implementation can still use:

```text
request.messages()
reflect.type_of<T>()
baml.schema.json_schema(...)
baml.http.send(...)
baml.sap.parse<T>(...)
```

No native wire capability is lost.

## LLM-function lowering migration

### Current conceptual lowering

```baml
baml.ai.drive_call<T>(
  client,
  Foo$render_prompt(args..., client = client),
)
```

### Proposed lowering

```baml
baml.ai.run<T>(
  Foo$request(args..., client = client),
)
```

`Foo$render_prompt` becomes a view over `Foo$request(...).prompt`. `$stream` uses the same request with compiler-supplied partial/final type parameters.

During migration, `$request` can wrap current prompt rendering and a compatibility `Generate` adapter can delegate to `drive_call`. This allows the public seam to land before every provider moves.

## Background migration

### Current usage

The current scenario calls the provider capability directly with a string and manually polls the same provider:

```baml
let job = provider.submit<Review>(prompt_text, key)
let result = provider.poll<Review>(job)
```

### Proposed usage

```baml
let job = baml.ai.submit_background(
  ReviewRepository$request(repo, client = provider),
  baml.ai.BackgroundOptions { idempotency_key: key },
)

let result = job.poll()
```

### Provider implementation change

The current `OpenAiResponses` implementation stores `Job { id, owner }` and re-parses on `poll`. It can be migrated mechanically into `OpenAiResponseJob<T>`:

```text
Job.id             -> OpenAiResponseJob.response_id
Job.owner          -> OpenAiResponseJob.owner: OpenAiResponses
provider.poll(job) -> OpenAiResponseJob.poll()
parse<T>           -> retained in the job implementation/owner helper
```

The existing real background endpoint remains useful as the first resource implementation and live conformance test.

## Sessions, chains, and caches

The branch currently splits state across provider methods and data handles:

```text
Conversational.chat(provider, prompt, Session)
Compaction.window_of(provider, Session)
Branching.fork(provider, Session)
Chain.extend(provider, prompt, ChainHandle)
ManagedCache.delete_cache(provider, CacheHandle)
```

BEP-063 moves behavior onto provider-owned resources:

```text
Session.run(request)
Session.compact(policy)
Session.fork()
CacheResource.run(request) or request option binding
CacheResource.delete()/cleanup()
```

Separate handles may remain as serializable tokens, but ordinary in-process APIs use resource objects.

## Realtime migration

The branch currently passes a `Channel` into `Realtime.run(prompt: string, io)` and passes the same channel back into `LiveControl.cancel/truncate`.

The proposed `Realtime.open_live(request, channel) -> LiveSession` returns the object that owns provider session state. `cancel_response()` and `truncate_assistant_audio()` operate on that object. The channel remains an input/output adapter, not the identity of the provider session.

## Combinator migration

### Retry

Current `Retry` wraps a `Provider`, consults `inner.is_effectful()`, and implements `HttpProvider`/`Streaming` with many forwarding or throwing methods.

Proposed retry wrappers are capability-specific:

```baml
class RetryGenerate {
  inner: Generate,
  policy: RetryPolicy,
  implements Provider {}
  implements Generate { ... }
}
```

Background retry is a separate implementation that requires an idempotency key. Realtime retry is absent unless a concrete resumable protocol is designed.

### Fallback

The proposed wrapper applies local metadata projection after provider selection and uses per-error commit state. It does not catch arbitrary user exceptions as member failures.

### Tracing

Tracing wraps returned resources so poll/cancel/session turns remain visible. Recording only the initial provider call is insufficient for stateful lifecycles.

## Capability introspection migration

The branch `Capabilities` interface exposes:

```text
structured_output
image_input
parallel_tools
max_input_tokens
```

BEP-063 keeps graded descriptors but makes them secondary:

- interface implementation answers whether an operation exists;
- `support(feature, request_shape)` answers model/payload-dependent questions;
- the actual call remains authoritative;
- descriptors may be cached/versioned provider metadata and must not be treated as a security boundary.

## Compatibility plan

### Stage A: Add requests without removing companions

- Generate `$request` for every LLM function.
- Implement it using the existing prompt closure/render path.
- Make `$render_prompt` delegate to `$request.prompt`.
- Keep existing drivers and generated companions.

### Stage B: Add `Generate` adapters

- Add `Generate` and `LlmResponse<T>`.
- Adapt `HttpProvider` values through a compatibility class.
- Lower main LLM calls through `run(Foo$request(...))`.
- Verify request/render/build parity.

### Stage C: Migrate leaf providers and wrappers

- Move built-in provider generation into semantic `Generate` methods.
- Retain reusable private codec helpers.
- Rewrite retry/fallback/tracing against semantic capabilities.

### Stage D: Migrate standard advanced operations

- Change streaming/tools/background/realtime drivers to accept `LlmRequest<T>`.
- Keep old companions as wrappers around new drivers during one deprecation window.
- Add resource objects for background/session/cache/realtime state.

### Stage E: Deprecate custom registry markers

The compiler can offer a mechanical diagnostic:

```text
`//baml:llm_companion(moderated)` is deprecated.

Export `drive_moderated(request: LlmRequest<T>, ...)` and replace
`Foo$moderated(args..., extras...)` with
`drive_moderated(Foo$request(args...), extras...)`.
```

Because both old and new driver bodies already perform runtime interface matches, migration is mostly signature/call-site rewriting.

### Stage F: Remove registry synthesis

- Delete marker parsing and capability registry queries used only for companion generation.
- Delete user-driver signature conventions for `client`, `prompt`, `T`, and `TPartial`.
- Remove cross-package copied-signature path rewriting.
- Retain ordinary interface metadata for tooling.

## Compiler/code paths expected to change

### Add or reshape

- AST/PPIR companion generation: add `$request`.
- LLM body lowering: main call and `$stream` use `$request`.
- `PromptAst`/request runtime representation: retain provider, `T`, identity, args, options, tags, and possibly a private re-render recipe.
- host codegen: typed request builders.
- `baml describe`: request and capability presentation.

### Eventually remove

- `llm_capability` marker parsing when no other tooling depends on it;
- `llm_companion_suffix` marker parsing;
- HIR `capability_registry` driver collection;
- PPIR per-user-driver companion generation;
- copied extra-generic/parameter substitution conventions;
- session-wide suffix conflict diagnostics.

### Stdlib files to reshape

- `ns_ai/core/provider.baml`;
- `ns_ai/core/messages.baml`;
- `ns_ai/core/meta.baml`;
- `ns_ai/capabilities/http.baml`;
- all advanced capability files to consume `LlmRequest<T>`;
- `ns_ai/combinators.baml`;
- provider implementations;
- `ns_llm/llm.baml` and `llm_types.baml` for request construction.

## Tests to preserve while migrating

The current branch has valuable fixtures that should be adapted, not discarded:

- roles preserved on the wire;
- native media encoding;
- OpenAI/Anthropic/Gemini structured parsing;
- strict JSON-schema mode;
- tool IDs and typed args;
- partial structured streaming;
- retry/fallback call counts and projection isolation;
- OpenAI Responses server chain;
- OpenAI Responses background submit/poll;
- realtime event and control flows;
- user class implementing a stdlib interface;
- user-defined capability dynamic negotiation;
- provider override on generated LLM-function surfaces.

The user-defined capability fixture should be rewritten to prove the new thesis:

```baml
run_moderated(ComposeNote$request(...), policy)
```

and assert that adding the driver creates no `ComposeNote$moderated` symbol.

## Expected deletion payoff

After migration, the compiler no longer needs to understand user capability driver signatures. The framework becomes:

```text
compiler responsibility:
  LLM function -> typed LlmRequest<T>

library responsibility:
  LlmRequest<T> -> any capability protocol

provider responsibility:
  implement supported protocol methods
```

That boundary is the main change from the branch design.
