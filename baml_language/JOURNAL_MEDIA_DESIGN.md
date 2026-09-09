# Media in the journal: carrier bridging, content blocks, and per-client lowering

Status: design for PR #4746 (`aaron/journal-user-content`). The branch's one commit carries an earlier shape (`ai.events.UserContent`) that this document supersedes; see §10 for the state of the branch.
Scope: the `baml_type` / `baml_compiler2_hir_ty` bridging of the media carrier classes, the `ai.content` / `ai.events` turn schemas, and every provider client's journal lowering.
Baseline: `canary` at 5f8cc28af.

## 1. Outcome

The final change should do all of the following:

1. Make `image`, `audio`, `video`, and `pdf` and their carrier classes `baml.media.Image`, `baml.media.Audio`, `baml.media.Video`, `baml.media.Pdf` denote the same type in every position, the way `baml.Int` already denotes `int`. `TyKind::Media` stays in the `Ty` family; the class spelling bridges *to* it.
2. Give every journal turn the same content vocabulary: `ai.content.ContentBlock = Text | Media` is what any turn may carry, and `ai.content.Block = ContentBlock | Reasoning | ToolUse | Refusal` is what a model turn may additionally produce. `ai.events.UserMessage { content: ContentBlock[], metadata }` and `ai.events.ToolCompleted { id, content: ContentBlock[] }` replace the text-only events; `ai.events.AssistantMessage { content: Block[] }` is unchanged. Remove `ai.events.UserContent`.
3. Lower those blocks in every provider client through one block lowering per client, with an explicit, tested outcome per (block kind, position): Accept, Degrade, or Reject.
4. Keep generated media an output-only block; reasoning round-tripping is a separate change (§8).
5. Let a runner append a screenshot after a tool result with one line, `journal.append_all([ai.events.UserMessage.of([caption, screen])])`, so a computer-use agent's prompt template can be static and the provider's prefix cache can hold it.

The final change should **not** remove `TyKind::Media` from the type family, should not introduce a second user-turn event or a second content vocabulary beside `ai.content.Block`, and should not change any generated SDK type or wire format.

## 2. Problem

### 2.1 An agent cannot add an image after the first turn

The journal (`ai.Journal`, the event log a runner appends to imperatively while it loops) can only carry text: `UserMessage { content: string }` and `ToolCompleted { id, output: string }`. Media enters a request only through the prompt template, which every client re-renders at the top of `contents` on every turn.

A computer-use runner that captures the screen after every tool call therefore has no place to put the capture. The workaround in use today is a mutable object rendered in the template's first user message and overwritten after every action. That has two costs:

- **No prompt caching.** Every turn's request differs from the first content part onward, so nothing after the system instruction can hit a prefix cache while the journal grows every turn. A fifteen-turn run re-sends the whole conversation fifteen times at full price (measured: ~175k uncached input tokens for one short workflow).
- **The model misreads it.** A tool result says a new screenshot exists, but nothing new appears after the tool result; the fresh image sits where the *starting* screenshot was. Gemini 3.8 has concluded that no screenshot was delivered and stopped.

### 2.2 `image` and `baml.media.Image` are two types

With the current CLI:

```baml
let img = image.from_url("https://example.invalid/a.png", "image/png");
let a: ai.PromptPart = img;        // error: expected `ai.PromptPart`, found `image`
let b: baml.media.Image = img;     // error: expected `baml.media.Image`, found `image`
function f(w: baml.media.Image) -> image { w }   // error: expected `image`, found `baml.media.Image`
```

while every other carrier already bridges:

```baml
function int_carrier(x: baml.Int) -> int { let y: baml.Int = 1; let z: int = y; x + z }   // ok
function string_carrier(s: baml.String) -> string { s }                                  // ok
function array_carrier(a: baml.Array<int>) -> int[] { a }                                // ok
```

The wrapper adds nothing. The primitive already has `url()`, `file()`, `mime_type()`, and `base64()`; `baml/ns_media/media.baml` declares `class Image { _data: $rust_type, ... }` as the class whose statics (`from_url`, `from_base64`, `from_file`) return `image` and whose instance methods are those accessors. Method resolution already looks the class up for a `Media` receiver, which is why `img.url()` typechecks. Only type identity is missing.

Because `ai.PromptPart` is spelled over the classes, the current PR needed `ai.internal.media_part`, a JSON round trip through the BEP-038 envelope, to accept an `image` where a `PromptPart` is expected. `ai.content.Media.new` already did the same. Both exist only because of this gap.

## 3. Type system: media carriers bridge to the `Media` kind

### 3.1 What TYPE_SYSTEM.md prescribes

`TYPE_SYSTEM.md` §Concrete Types describes two shapes of builtin companion:

- **Carrier classes** (`baml.Int`, `baml.String`, `baml.Array<T>`, …) "effectively do not exist as a type": every occurrence of the spelling denotes the builtin type itself, in every position, including `Self` inside the carrier's own methods. No value inhabits the nominal class.
- **Dedicated-kind companions** (`reflect.Type`, `baml.future.Future<V, E>`): the class name *is* the builtin kind's canonical spelling; the class declaration exists to carry members and documentation.

The media classes are listed under neither. They belong to the second shape: `Media(MediaKind)` is a dedicated leaf kind in the `Ty` family (Borsh tag 7), with its own VM object, its own BEP-038 `{kind, source, value, mime}` envelope at the FFI boundary, its own SDK codegen mapping (`BamlImage` in Python, and the equivalents in every generator), its own prompt-renderer semantics (`${img}` splits a message into parts), and its own `Category::Media` for disjointness in `normalize.rs`. The class `baml.media.Image { _data: $rust_type }` carries its members. This document adds media to that second list in `TYPE_SYSTEM.md`.

### 3.2 Decision: keep `Media` in the family; bridge the class spelling

The alternative that was started and abandoned (§10) was the `json` route: lower the `image` keyword to the path `baml.media.Image`, delete `TypeExprKind::Media` / `TyKind::Media` / `MediaKind`, and have every consumer re-derive media-ness from the class path via `builtin_primitive()`. It is the wrong analogy. `json` is a *structural alias* (`type json = null | bool | int | float | string | json[] | map<string, json>`) with no runtime kind of its own, so aliasing costs nothing. Media is a concrete runtime kind with a memory layout, a wire tag, a codegen projection, and renderer behaviour; turning it into a nominal class with an opaque field would push a leaf kind's identity into a string comparison on a class path inside the VM, the bridge, every codegen backend, the renderer, and the SAP parser. The attempt touched 37 files across `baml_type`, `hir`, `hir_ty`, `mir`, `ppir`, `emit`, codegen, and the IDE before it was stopped, and it would have violated the guideline that types exist at runtime with accurate reflection.

The `json` precedent that *does* apply is narrower: `lower_type_expr.rs` gives keywords a single canonical resolution. `image` already has one (`TypeExprKind::Media`, then `TyKind::Media`). The class spelling is what lacks it.

### 3.3 The fix: one arm in `class_ty`

`crates/baml_compiler2_hir_ty/src/lower.rs` `class_ty(qtn, args)` is "the single constructor for class types, shared by annotation lowering and `class_self_ty`", and it already applies the builtin bridgings uniformly: `baml.future.Future<V, E>` → `TyKind::Future`, `baml.Array<T>` → `List`, `baml.Map<K, V>` → `Map`, and the scalars and `Null` → their kinds (B-1080). Media joins that list:

```rust
// class_ty, after the Future arm:
if args.is_empty()
    && let Some(primitive) = qtn.builtin_primitive()   // baml.media.Image -> PrimitiveType::Image, baml.Int -> Int, ...
{
    return Ty::from_primitive(primitive, attr());       // lib.rs already maps PrimitiveType::Image -> Ty::Media(MediaKind::Image, _)
}
```

`QualifiedTypeName::builtin_primitive()` (`baml_type/src/names.rs`) and `PrimitiveType::builtin_class_path()` (`baml_type/src/primitive.rs`) already map `["media", "Image"]` ↔ `PrimitiveType::Image`, and their own doc comment calls this "the single collapse rule". Routing the scalar arms through the same registry replaces the hand-written `"Int" | "Bigint" | …` match, so the carrier family stays total by construction (`Null` included) and cannot drift again.

Because `class_self_ty` calls `class_ty`, `self` inside `class baml.media.Image` becomes `image` with no further change, exactly as `self` in `baml.Array<T>` is `T[]`.

### 3.4 Consequences to verify

- **Method resolution** (`method_resolution.rs`, the B-1080 comment near line 1328): a `Media` receiver already resolves members on the carrier; confirm the reverse direction has no remaining nominal-class path.
- **Operator dispatch** (`infer.rs` `operand_members` ~line 13622): the `widen` arms cover the scalars only; add the media kinds for uniformity, or replace the match with `builtin_primitive()` + `Ty::from_primitive`.
- **Construction** (`baml_type/src/type_kind.rs` `builtin_companion_of`): today scoped to the `baml` root namespace and explicitly excludes `baml.media.Image` (its test lists it as "should not be a companion carrier" because it holds a field). After bridging, `baml.media.Image { _data: … }` from user code must still be rejected; extend the companion registry to the media classes (origin: "`image.from_url`, `from_file`, `from_base64`") or confirm the `$rust_type` field already makes the literal unconstructible with a clear diagnostic.
- **Reflection**: `reflect.Type.of<baml.media.Image>()` must equal `reflect.Type.of<image>()`.
- **IDE**: hover, describe, and completion already print `image` via `builtin_alias()`; verify nothing prints the class spelling.
- **Codegen and wire**: unchanged by construction, since both spellings now produce the same `Ty`. The snapshot suites (`bytecode.snap`, `mir.snap`, `ppir.snap`) should not move except where a type used to print as the class.
- **`ai.internal.media_part` and the JSON round trip in `ai.content.Media.new`**: delete; take the value directly.

### 3.5 Tests

A new namespace `crates/baml_tests/baml_src/ns_media_alias/` covering, for each of the four kinds: assignability in both directions; the keyword inside unions (`string | image`), arrays, maps, optionals, class fields, function params and returns, and generic arguments; `ai.PromptPart` accepting `image.from_url(...)` directly; matching a rendered prompt part with both `let i: baml.media.Image = part` and `let i: image = part`; statics and accessors; `baml.json.to_json(img)` unchanged; reflection equality; and a rejected class literal. Plus a unit test on `class_ty` asserting every `PrimitiveType::ALL` member's class path bridges to the same `Ty` as its alias.

## 4. Event schemas: one block vocabulary for every turn

### 4.1 The inconsistency being removed

Text and media are carried three different ways today, and the first draft of this design added a fourth:

| | Text | Media | Field | Where |
|---|---|---|---|---|
| Rendered prompt | bare `string` | bare `image` | `PromptMessage.parts: PromptPart[]` | Rust renderer |
| User turn, tool result (first draft) | bare `string` | bare `image` | `parts: Part[]` | journal |
| Assistant turn (existing) | `Text { text }` | `Media { media, provider_id?, revised_prompt? }` | `content: Block[]` | journal |

Inside the journal that is two representations of the same two concepts under two field names, and every client would need one lowering path for parts and another for blocks even though they carry the same payloads. The assistant side wraps for a reason: `Media` carries provenance for generated output, and `Text` exists so text can sit in a union beside `Reasoning` and `ToolUse`. That reason applies to every turn, so every turn uses blocks.

### 4.2 Types

```baml
// ai/ns_content/content.baml
class Text  { text: string }
/// `provider_id` and `revised_prompt` are provenance of generated output and
/// are null on input. `provider_id` is superseded by `image.from_ref` (§8)
/// and should not be read by new code.
class Media { media: MediaPart, provider_id: string?, revised_prompt: string? }
/// What any turn may carry.
type ContentBlock = Text | Media;
/// What a model turn may additionally produce.
type Block = ContentBlock | Reasoning | ToolUse | Refusal;

// ai/ns_events/events.baml
class UserMessage {
    content: ContentBlock[],
    /// Per-message provider directives, carried verbatim like
    /// `ai.PromptMessage.metadata` (Anthropic `cache_control`, for example).
    metadata: map<string, baml.json.json>,

    function new(text: string) -> UserMessage
    function of(
        items: (string | image | audio | video | pdf | Text | Media)[],
        metadata: map<string, baml.json.json> = {},
    ) -> UserMessage
    /// The `Text` blocks joined; media blocks contribute a `[image]`-style placeholder.
    function text(self) -> string
}

class ToolCompleted {
    id: string,
    content: ContentBlock[],

    function new(id: string, output: string) -> ToolCompleted          // the tool's JSON text
    function of(id: string, items: (string | image | audio | video | pdf | Text | Media)[]) -> ToolCompleted
    /// The `Text` blocks joined: the tool's text output.
    function text(self) -> string
}

class AssistantMessage { content: Block[], client_id: string }      // unchanged
// ToolFailed { id, message } is unchanged: an error, not content. UserContent is removed.
type Event = RunStarted | UserMessage | AssistantMessage | ToolRequested
           | ToolCompleted | ToolFailed | Usage | LLMCall | FinalProduced;
```

`Refusal { text: string }` is listed for completeness of the `Block` union (§8); it may land with this change or after it.

Every turn has a `content` list of blocks. `Block` is a strict superset of `ContentBlock`: the blocks any role may carry, plus the assistant-only ones. Convenience lives in the constructors, so a caller still writes `UserMessage.of([caption, screenshot])`; with §3 in place that typechecks without a shim, and `of` wraps a bare `string` in `Text` and a bare media value in `Media` with null provenance.

`ai.PromptPart = string | MediaPart` stays as the renderer's raw payload for `PromptMessage.parts`. That shape crosses the Rust boundary and is documented as portable (FUNCTION_SPEC_STREAMING_DESIGN.md §7), and it is the one place bare values are right: it is what the provider sees, with no provenance. It does not appear in the event catalog. Moving the renderer's output onto blocks is possible later but is a runtime change, not a schema one.

### 4.3 Why this shape

It is the model OpenRouter uses to front every provider with one schema, and it maps onto ours directly:

| OpenRouter | BAML |
|---|---|
| `content: string \| ContentPart[]` on `user` messages; parts `text`, `image_url`, `input_audio`, `file`, `video_url` | `UserMessage.content: ContentBlock[]` |
| `role: tool` with `content: string \| array` | `ToolCompleted.content: ContentBlock[]` |
| assistant `content` plus sidecars `tool_calls`, `reasoning_details`, `images`, `audio`, `refusal` | `AssistantMessage.content: Block[]` (blocks, not sidecars: ordering matters for Anthropic and Gemini) |
| generated `images` never accepted on an input assistant message | `Media` in an assistant turn is output; a follow-up edit re-sends it as a user block |

Specifically:

- **One event for a user turn, not two.** `UserContent` beside a text-only `UserMessage` leaves every client and listener handling both, with the text case as the odd special case.
- **One vocabulary, not two.** Each client already matches `Text` and `Media` when replaying assistant turns (`_anthropic_assistant_blocks` and its equivalents). With every turn on blocks that becomes a single `lower_blocks(blocks, position)` per client, used for user turns, tool results, and assistant turns.
- **Tool results as blocks.** Every provider except OpenAI Chat Completions accepts media inside a tool result (§5), and a screenshot *is* the result of a computer-use tool. The text-only providers degrade rather than reject.
- **`metadata` on user turns.** Once the prompt template is static, the last stable turn is a journal event, and that is where an Anthropic cache breakpoint belongs.
- **Assistant media stays output-only.** Anthropic, Chat Completions, and Bedrock reject media in assistant messages; Responses replays generated images by item id and Gemini by model-role `inlineData`. Those two replays are client work that does not touch the event catalog (§8).

### 4.4 `ai.wire.sanitize_for_client`

The `UserMessage` arm keeps flushing orphaned tool calls before the turn. A `ToolCompleted` with content resolves its pending call exactly as before. A helper `ai.wire.blocks_prompt(role, blocks, metadata) -> ai.PromptMessage` (`Text` to `string`, `Media` to its media value, `content` = the text projection) lets every client reuse its existing prompt helper for the positions where that is the natural lowering; it replaces the current `user_content_message`.

## 5. Client lowering: Accept, Degrade, Reject

Definitions, applied per (block kind, position), where position is one of *user turn*, *tool result*, or *assistant turn*:

- **Accept**: lower natively at that position.
- **Degrade**: keep the text where it is and move the media into a trailing `user` message with a one-line note ("attached from the result of tool `x`"). Lowering stays pure: no journal mutation, no event emission. The trace still shows the original event.
- **Reject**: a typed client-side error before any request is sent, as `_anthropic_prompt_blocks` does for audio today.

Each client lowers all three positions through one `lower_blocks(blocks, position)` and declares its table in one comment block beside it. Each cell is covered by a test that seeds a journal with an assistant tool call, a `ToolCompleted.of` carrying an image, and a `UserMessage.of` with text plus an image, then asserts where the image landed and that text-only journals lower byte-identically to before. The assistant column keeps today's behaviour (generated `Media` is dropped on replay) until §8's replay-by-reference work.

Expected tables, to be verified against current provider docs before coding:

| Client | user image | user audio | user video | user pdf | tool-result media |
|---|---|---|---|---|---|
| Gemini (`google/ns_internal/gemini.baml`, and Vertex) | Accept | Accept | Accept | Accept | Accept via `functionResponse.parts`; Degrade if the target API version rejects it |
| Anthropic (`anthropic/ns_internal/messages.baml`) | Accept | Reject | Reject | Accept | Accept via `tool_result.content` blocks (image; document if allowed there, else Degrade) |
| OpenAI Responses (`openai/ns_internal/responses.baml`) | Accept | Reject | Reject | Accept | Accept if `function_call_output.output` takes input parts, else Degrade |
| OpenAI Chat (`openai/ns_internal/chat.baml`) | Accept | Accept | Reject | Accept | Degrade (tool content is text-only) |
| Bedrock Converse (`aws/ns_internal/bedrock.baml`) | Accept | Reject | Accept | Accept | Accept via `toolResult.content` blocks |
| Claude Code CLI transcript (`claude_code/ns_internal/cli.baml`) | placeholder text | placeholder | placeholder | placeholder | placeholder |
| Images clients | Reject | Reject | Reject | Reject | Reject |

Journal lowerers that do not currently receive the options their prompt helpers need get them threaded from the body builder: `google_lower_journal(j, fetch_url, preview)`, `chat_lower_journal(j, compat, preview)`, and the Anthropic, Responses, and Bedrock equivalents take `preview`.

## 6. Runner and agent usage

`ai/runner.baml`'s default agent loop records tool results with `ToolCompleted.new(id, output)`; nothing else changes for it. A runner that observes the environment after each turn appends the observation as a user turn after the tool results:

```baml
// after dispatching the turn's tool calls
journal.append_all([
    ai.events.UserMessage.of([
        `Screenshot ${n} of this run, captured after your ${action} completed; ${changed}% of pixels differ from screenshot ${n - 1}.`,
        host.screenshot(),
    ]),
]);
```

The prompt template then carries only the system instruction and the starting screenshot, so the request prefix is byte-identical from turn to turn, the journal is append-only, and the uncached portion of each turn is the latest tool result plus one image. The model reads the newest screenshot in the conversation as the current screen, at the position where it expects new information.

## 7. Breaking changes and migration

| Before | After |
|---|---|
| `ai.events.UserMessage { content: "..." }` | `ai.events.UserMessage.new("...")` |
| `user.content` (a `string`) | `user.text()` for the projection; `user.content` is now `ContentBlock[]` |
| `ai.events.ToolCompleted { id, output }` | `ai.events.ToolCompleted.new(id, output)` |
| `completed.output` | `completed.text()` for the JSON text; `completed.content` for structure |
| `ai.events.UserContent` (PR #4746 as committed) | `ai.events.UserMessage.of(items)` |
| `ai.internal.media_part(value)` | the value itself |

Any `on_event` listener with an exhaustive `match` on `ai.events.Event` compiles unchanged (the union loses `UserContent` and gains nothing). A `match` on `ai.content.Block` gains a `Refusal` arm if that block lands here. Every constructor and reader across `crates/` is updated in the PR; `grep -rn "ai.events.UserMessage {\|root.events.UserMessage\|ToolCompleted {\|\.output\b"` finds them.

## 8. Deferred follow-ups

Named here so the PR can point at them; none changes §4's catalog.

- **Reasoning round-trip.** `ai.content.Reasoning { summary }` discards provider signatures, so every client drops reasoning on replay and Gemini smuggles its `thoughtSignature` through `ToolUse.id`. Adopt OpenRouter's `reasoning_details` shape: `summary?`, `text?`, `signature?`, `encrypted?`, `provider_id?`, `format`, replayed verbatim and in order by the client whose `format` it is. Also `ToolUse.signature: string?`. This touches the streaming decoders.
- **`Refusal { text }` block**, the `refusal` sidecar.
- **Provider references as a media source.** `image.from_ref(provider, id, mime?)` and `img.ref()` beside url, file, and base64, so an OpenAI file id, a Gemini Files-API URI, or an `image_generation_call` id flows back in as an ordinary `Media` block. Extends the BEP-038 envelope's `source`, and retires `Media.provider_id`, which then duplicates the value's own reference.
- **Assistant media replay by reference**: Gemini model-role `inlineData`; Responses `image_generation_call` by id; Chat Completions generated audio by `audio.id`. Everyone else keeps dropping it.
- **`modalities` on `ai.ModelTurnInput`** (OpenRouter `modalities: ["text", "image"]`) instead of inferring image output from the return type in the Responses client.
- **Per-part hints** such as OpenAI image `detail`: client options or message `metadata`, not fields on the media type.

## 9. Validation

From `baml_language/` (per `~/.claude/CLAUDE.md`: nextest, never `cargo test`; insta through nextest with `--dnd`; never commit `.snap.new`):

```
cargo build -p baml_cli                                     # target/debug/baml-cli
target/debug/baml-cli check  --project crates/baml_tests/baml_src
target/debug/baml-cli test   --project crates/baml_tests/baml_src        # whole offline corpus
cargo insta test --test-runner nextest --dnd -p baml_tests -p baml_cli -p baml_lsp2_actions --all-features --unreferenced=reject
cargo nextest run --all-features --workspace --exclude baml_tests --exclude baml_cli --exclude baml_lsp2_actions --exclude "sdk_test_*" --exclude baml_bridge
```

Snapshot policy: §3 should move no snapshot except where a type used to print as the class spelling; §4 moves the per-namespace `bytecode.snap`/`mir.snap` that spell out the `Event` union and the `stdlib/ai` snapshots. Every accepted snapshot gets a one-line reason in the PR body. One pre-existing flake is known: `baml_tests::shell claude_code_client_preserves_process_wait_timeout` fails on base `5f8cc28af` alone (a 25 ms race).

The downstream check is the Epic computer-use agent at `apacendo-rpa/agents/epic_agent`: regenerate with the built CLI, run its BAML and Python suites, and confirm on a live run that `cached_input_tokens` is non-zero from the second turn on.

## 10. State of the branch

- Commit `1c1342347` on `aaron/journal-user-content` implements the superseded `UserContent` shape with the `media_part` shim, plus lowering arms and tests in every client. The lowering arms and tests are reusable once re-pointed at `UserMessage.of` / `ToolCompleted.of` and folded into each client's single `lower_blocks`.
- The working tree additionally holds an uncommitted 37-file attempt at the rejected alternative in §3.2 (removing `Media` from the `Ty` family). Discard it (`git checkout -- .` in the worktree) before starting on §3.
- Order of work, each as its own commit on this branch: §3 with §3.5 tests and snapshots green; §4 and §5 with their tests green; `TYPE_SYSTEM.md` and the PR body last.
