# Implementation status

Where the BEP's implementation actually stands, as of the end of the
`aaron/custom-llm-providers-v5` work. This is a working document, not part
of the BEP proper: it records what shipped, what is half-done, and the
decisions still open. `readme.md` describes the design; this describes
reality.

## Branch and commit state

| | |
|---|---|
| PR | #4352 against `canary` |
| Branch | `aaron/custom-llm-providers-v5` |
| Last green pushed commit | `b798e934f` |
| Current worktree | `baml.llm` deletion plus the `ai.stream` host-ABI migration (not committed yet) |

The pushed branch head is green. The current worktree carries the deletion and
stream migration described below; the compiler/runtime, `sys_llm`, and the
`_planv2` offline suite build and pass together.

## What shipped

### LLM functions execute through one path

Every LLM function desugars to the same thing:

```
Fn(args)  ==>  ai.Agent<Out>.new(client = client).run(Fn@spec(args)).value
```

There is no second path. The legacy `client<llm>` block, the Jinja prompt
pipeline, and `baml.llm.call_llm_function` are gone; what replaced them is
BAML source in the `ai` package and the four provider packages. Anything the
old syntax supported that the new path does not (Bedrock, Vertex, Azure)
raises a migration error at compile time rather than silently degrading.

### Companions

`@spec` reifies a call without running it. The compiler synthesises four
companions per LLM function:

| Companion | Returns | Level |
|---|---|---|
| `Fn$spec` | `ai.FunctionSpec<Out>` | AST |
| `Fn$render_prompt` | `ai.Prompt` | AST |
| `Fn$parse` | `Out` | AST |
| `Fn$stream` | `ai.stream.Stream<OutStream, Out>` | PPIR |

`$stream` is synthesised in PPIR rather than lowered as an item, which is why
it never appears in a `baml describe` item listing.

### Prompts are structural

`FunctionSpec.prompt(output_format = text)` returns the Rust-backed
`ai.Prompt`. The compiler builds it with the same tagged-template
assembler as the public `prompt` tag, so `${role(...)}` creates ordered
messages and media remains structural. OpenAI, Anthropic, and Google lower
those messages into their respective request shapes; Claude Code projects the
AST to text only at its single-string CLI boundary.

### Clients are values

`client Name = <expr>;` is an ordinary top-level binding evaluated during
`$init`. Because `$init` cannot run io sysops, a client declaration names its
credentials rather than reading them:

```baml
client Stub = openai.OpenAiClient.new(
  model = "gpt-5.4-nano",
  api_key_env = "OPENAI_API_KEY",
  base_url_env = "OPENAI_BASE_URL",
);
```

The provider resolves those at request time. This is the single most
load-bearing detail for anyone writing a fixture: reading `env.X` directly in
a client declaration compiles, then fails at runtime with
`$init function yielded unexpectedly: SysOp { operation: BamlEnvGet }`.

Retry, fallback, and round-robin are not special syntax — they are clients
that compose other clients (`ai.clients.Retry`, `Fallback`, `RoundRobin`).

### Streaming

Streaming shipped, though the BEP originally filed it under future phases.
A client opts in by implementing `ai.stream.StreamingClient`, whose
`invoke_stream` returns a pullable `ai.stream.TurnStream` folded from
provider `StreamEvent`s. This differs from the BEP's sketch, which had an
invoke variant feeding an ephemeral sink.

Host-language streaming uses one canonical type:
`ai.stream.Stream<TPartial, TFinal>`. Tagged handles carry that concrete class
identity across FFI; host wrappers derive `next` / `final` from the carried
identity rather than hardcoding a namespace. SDK generators share the
well-known stream identity when mapping it to each language's stream wrapper.

## The `ai` namespace

`ai` had drifted to 45 items at the package root, grouped by implementation
file rather than by audience. It now matches the tree in `readme.md`:

```
ai                          the nine names the flow diagram uses
├── FunctionSpec  Prompt
├── Runner  Agent  RunResult
├── Client  ModelTurnInput  ModelTurn
└── Journal
    ├── ai.content     Text, Reasoning, ToolUse, Block, StopReason
    ├── ai.events      RunStarted … FinalProduced, Event, Usage
    ├── ai.tools       Tool, Toolbox, ErrorMode, tool(), raw_tool()
    ├── ai.clients     Retry, Fallback, RoundRobin, Backoff
    ├── ai.stream      StreamingClient, TurnStream, StreamEvent, Stream, Done, from_spec()
    ├── ai.wire        send_as, render_output_format
    ├── ai.errors      the failure taxonomy
    ├── ai.mcp         MCP servers as ordinary tools
    └── ai.internal    compiler-facing helpers
```

Four names changed, each a stutter only visible once things were qualified:
`ToolErrorMode` → `ai.tools.ErrorMode`, `ContentBlock` → `ai.content.Block`,
`stream_spec` → `ai.stream.from_spec`. `Usage` stays in `ai.events` per the
BEP, so a provider building a `ModelTurn` writes `ai.events.Usage`.

Divergences from the intended BEP surface, now recorded in
`baml_src/readme.md`:

- The provider clients are **root packages** (`openai.OpenAiClient`), not
  `ai.clients.*`. `ai.clients` holds only clients that compose other clients.
- There is no runtime `register` / `resolve`. A model string resolves through
  a compile-time map in the desugar, so an unknown prefix is currently a
  compile error. The BEP retains `ai.clients.resolve/register` as its intended
  public surface.
- `Media` as an output content block, `wire.closed_schema`, and
  `wire.strict_schema` were specified but are not implemented. The BEP retains
  output `Media`; this implementation gap does not remove it from the design.

## Current worktree: deleting `baml.llm`

Goal: `baml.llm` should not exist. Framework concepts live under `ai`, while
language primitives stay in their narrow `baml.*` namespaces.

### Done and sound

- `ns_llm/llm.baml` deleted. All ten declarations were legacy
  (`call_llm_function`, `build_request`, `build_request_stream`,
  `render_prompt`, `prompt_template_for`, `parse`, `__new_stream_cache`,
  and the `OrchestrationStep` / `ExecutionContext` / `PlannerState` planner
  triple). Every reference outside the file was a comment.
- `ns_llm/llm_types.baml` reduced from 780 lines to a 169-line
  `ai/ns_primitives/primitives.baml`, dropping 14 dead declarations: the
  legacy `Client`, `ClientType`, `RetryPolicy`, `PrimitiveClient`, all five
  provider option classes, `MediaUrlHandler`, `StreamAccumulator`,
  `build_prompt_context`, `from_shorthand`.
- `Stream` and the public `Done` sentinel moved into `ai.stream`, beside
  `TurnStream`. Incremental parser state is now the private
  `baml.sap.ParseCache` / `baml.sap.NoYield` pair. `baml.prompt` contains no
  streaming declarations.
- `crates/baml_compiler2_ast/build.rs` deleted — 235 lines that parsed
  `PrimitiveClientOptions` out of the BAML source to generate
  `client_fields_generated.rs`, **which nothing includes**. Dead
  infrastructure kept alive only by the classes it read.

### The codegen change that unblocked it

`$rust_io_function` and `$rust_type` bindings were hardcoded to the `baml`
package: `io_namespace_name` did `strip_prefix("baml.")` and the generated
dispatcher matched `Some(("baml", rest))`, returning `None` for anything
else. The `ai` package had never contained a single Rust-bound declaration.

`baml_builtins2_codegen` is now package-aware:

- `extract_native_builtins` merges `baml` and `ai` rather than scoping to
  `PACKAGE_BAML`.
- Namespace keys are `pkg.ns`, so `baml.stream` and `ai.stream` cannot
  collide.
- `ns_key` keeps `baml`'s historical unprefixed spelling, so existing trait
  names — and the ~40 hand-written `sys_ops` impls against them — do not
  churn. Only non-`baml` packages get qualified names.

`sys_types` builds and emits the package-qualified `ai` and `baml.sap` types.

### The handle/owned bug is fixed

The generated `RuntimeIo` trait previously became internally inconsistent:

```
fn fs_open(...)              -> FsFileHandle        // or owned::fs::File
fn fs_file_text(&FsFileHandle)                      // expects a handle
```

`codegen_io.rs` now resolves both sides through the same package-aware class
classification. `sys_types`, `sys_ops`, and `sys_llm` compile together; the
new SAP bindings are generated as `baml.sap.ParseCache` rather than leaking a
prompt- or stream-owned cache type.

## Streaming ownership

There is one streaming pipeline, with each layer owning one concern:

| Layer | Owner | Responsibility |
|---|---|---|
| Wire transport | `baml.http.SseStream` | Pull raw SSE frames from HTTP |
| Provider turn | `ai.stream.StreamEvent` / `TurnStream` | Decode text deltas and fold turn metadata |
| Typed function result | `ai.stream.Stream<TPartial, TFinal>` / `Done` | Incrementally parse partials and produce the final value |
| Parser internals | `baml.sap.ParseCache` / `NoYield` | Cache compiled schemas and signal “not parseable yet” |
| Host ABI | tagged heap handle | Carry the concrete stream class FQN and generic types over FFI |
| SDK bridge | language-native stream wrapper | Invoke `<carried FQN>.next` / `.final`; expose native sync/async iteration |

SDKs do not define a second BAML streaming protocol. Their wrappers adapt the
same tagged `ai.stream.Stream` handle to language conventions. Compiler
synthesis, engine handle lifting, and Rust SDK generators share
`baml_base::qualified_name::AI_STREAM_STREAM` and `AI_STREAM_DONE`; generated
host code uses that canonical identity and bridges retain the handle's carried
class FQN.

## `sys_llm` is mostly dead, but do not delete it yet

`sys_llm` is 21,694 lines: the entire pre-`ai` Rust pipeline. Its directories
are `jinja/`, `build_request/`, `parse_response/`, `specialize_prompt/`,
`auth_request/`, `resolve_media/`, `stream_accumulator.rs`, `provider.rs`,
`model_features.rs`.

Callers outside the crate:

| Entry point | External callers |
|---|---|
| `execute_render_prompt_from_owned` | 0 |
| `execute_specialize_prompt_from_owned` | 0 |
| `execute_parse_response_from_owned` | 0 |
| `execute_validate_finish_reason` | 0 |
| `render_output_format` | 13 |
| `execute_sap_parse_final` | 1 |
| `execute_sap_parse_partial` | 1 |

Only schema rendering and schema-aligned parsing are still reached; both
genuinely need the type system and the repair parser, so they stay.

**Zero callers from the `ai` path is not the same as dead.** During this work
~9,400 lines were deleted on that reasoning and then restored:

| Module | Lines | What it implements |
|---|---|---|
| `build_request/` (7 files) | ~6,500 | OpenAI chat-completions + responses + images, Anthropic, Google, Bedrock |
| `auth_request/vertex.rs` | 1,261 | GCP OAuth for Vertex AI |
| `auth_request/bedrock.rs` | 842 | AWS SigV4 |
| `resolve_media.rs` | 833 | Media URL download and encoding |

The BAML clients cover OpenAI Responses, Anthropic, Gemini, and Claude Code.
Bedrock, Vertex, Azure, and media resolution have **no** BAML equivalent.
Deleting that Rust drops provider support that exists nowhere else.

The sequencing decision: **remove the old Rust only after all clients are
migrated to BAML.** Until then it is dormant, not dead.

## Open work

1. **Migrate the remaining providers** (Bedrock, Vertex, Azure) to BAML
   clients, which is what unlocks deleting `sys_llm`'s request pipeline.
2. **Lazy client declarations.** Decided but unimplemented. Initializers
   already compile to standalone zero-arg `$init_let_{i}` functions with
   `Call`+`StoreGlobal` in `$init`, and there is exactly one read site
   (`emit.rs`, `Constant::GlobalItem` → `LoadGlobal`). The plan is a
   `$lazy_get_{fq}` per client let, a lazy map threaded through `CompileCtx`
   and `StackifyCodegen`, dispatch at that read site, and a cycle guard.
3. **Program size.** Adding the `ai` and provider packages grew every
   program's bytecode by ~368 KB (+38%), because `package_dependencies` puts
   all five in every user package's closure and emit has no dead-code
   elimination. Pre-existing behaviour — `canary` already ships `baml.llm`
   and `testing` into programs that never use them — but now material. DCE
   would put programs below where `canary` sits today.

## Notes for whoever picks this up

- **`ai.stream.Stream` is an SDK ABI type.** Its identity and `ai.stream.Done`
  are centralized for Rust synthesis/generation in `baml_base::qualified_name`.
  Host bridges must use the class identity carried by the tagged handle when
  invoking `next` / `final`; a bridge may still recognize the canonical `Done`
  identity when adapting exhaustion to its native iterator API.
- **`resolve_provider_options`** in `sys_llm/baml_std.rs` matches provider
  option class names with `unreachable!()` on anything else. A rename there
  panics at runtime rather than failing to build.
- **`baml.llm.Resource`** is referenced in `baml_type/src/lib.rs` as a
  `Display` string but has no class declaration anywhere. It is display-only.
- **Verify against the whole workspace.** `baml_lsp2_actions_tests` is a
  separate crate from `baml_lsp2_actions`, and the SDK probes live in
  `sdk_tests`. A three-package run misses both, and both broke during the
  namespace move.
- **`UPDATE_EXPECT=1` will happily bless errors.** During the namespace move
  it "fixed" six LSP fixtures by recording `unresolved name` diagnostics as
  the expected output — a green suite asserting the fixtures are broken.
  Always diff the regenerated expectations for `E000x` before committing.
