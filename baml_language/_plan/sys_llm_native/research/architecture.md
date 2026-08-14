# sys_llm → native BAML: wiring, dispatch, and target architecture

Research date: 2026-08-12. Repo: `/Users/aaron/projects/baml/baml_language` @ `canary` (593a51363).
Every claim below cites `file:line`. Paths are relative to the repo root unless prefixed `engine/`
(= `/Users/aaron/projects/baml/engine`).

---

## 0. Executive summary — the headline finding

**The LLM dispatch migration is already done. `sys_llm`'s provider stack is dead code.**

There is no `LegacyClient` bridge, no `//baml:llm_capability` annotation, no `_plan/llm-desugar-capabilities-plan.md`
in this repo, and no runtime path from a BAML program to `sys_llm::build_request` / `parse_response` /
`auth_request` / `specialize_prompt` / `stream_accumulator` / `resolve_media`. Those symbols have **zero
callers outside `sys_llm`'s own `#[cfg(test)]` modules**.

Evidence:

- The only crate that references `sys_llm` in Rust source is `sys_ops`
  (`crates/sys_ops/src/lib.rs:160, 191, 715, 727, 765, 784, 798, 809, 813`), and it uses exactly four things:
  `render_output_format`, `render_output_format_content`, `build_output_format_content`,
  `SapParseCache` + `execute_sap_parse_final` / `execute_sap_parse_partial`.
- `bex_engine/Cargo.toml:44` lists `sys_llm` only under `[dev-dependencies]` (section starts at line 40),
  and no file under `crates/bex_engine/` actually names it — it is a stale dev-dep.
- `crates/bex_cache/Cargo.toml:13` mentions `sys_llm` only in a comment about TLS feature selection.
- `crates/sys_ops/Cargo.toml:12-13` forwards the `aws-crypto`/`ring-crypto` features to `sys_llm`; that is
  the only other coupling and it is vestigial (`crates/sys_llm/src/lib.rs:31-38` documents that
  `ensure_rustls_crypto_provider` is "no longer called directly").
- `crates/sys_llm/src/lib.rs:313, 326, 342, 521, 542` define `execute_specialize_prompt_from_owned`,
  `execute_build_request_from_owned`, `execute_build_request_stream_from_owned`,
  `execute_validate_finish_reason`, `execute_parse_response_from_owned`. Grepping the whole workspace for
  those names returns only `crates/sys_llm/src/lib.rs` itself (definitions + the `mod tests` block at
  lines 996–1461).

So the migration plan is **not** "port sys_llm to BAML". It is:

1. **Delete** the ~15k dead lines of `sys_llm` (phase 2), and
2. **Close the parity gaps** the native `ai`/provider packages still have versus what `sys_llm` and the
   old engine could do — chiefly *media in prompts*, *non-Responses OpenAI wire formats*, and
   *Vertex/Bedrock auth*, which are the only places new Rust seams are legitimately needed.

---

## 1. Dispatch path today

### 1.1 `client<llm>` config blocks are a hard compile error

`crates/baml_compiler2_ast/src/lower_cst.rs:128-138` — a `CLIENT_DEF` CST node lowers to exactly one
diagnostic and no items:

```rust
baml_compiler_syntax::SyntaxKind::CLIENT_DEF => {
    // Legacy `client<llm> Name { ... }` config block: removed in
    // the single-path world. ...
    diags.push(LoweringDiagnostic::ClientBlockRemoved { name, span: child.span_range() });
}
```

The message is at `crates/baml_compiler2_ast/src/lowering_diagnostic.rs:560-570`:
"``client<llm>`` config blocks are removed; declare a client value instead: `client {name} = openai.OpenAiClient.new(model = "...")`".
`retry_policy` blocks get the same treatment (`lowering_diagnostic.rs:571-583`), pointing at `ai.Retry`.
`crates/baml_compiler2_ast/src/lower_cst.rs:2172-2177` is the tombstone comment for the deleted synthesis code.

The parser still *recognizes* `client<llm> Name { … }` (`crates/baml_compiler_parser/src/parser.rs:7390-7406`,
`1147-1151`) purely so the error is one targeted diagnostic instead of a cascade.

### 1.2 `client Name = <expr>;` is the replacement

`CLIENT_VALUE_DEF` lowers to an ordinary `let` item (`crates/baml_compiler2_ast/src/lower_cst.rs:121-127`,
`lower_client_value_def`). Example fixture: `crates/baml_tests/projects/compiles/o1_allowed_roles/o1_clients.baml:5-18`
— `client O1Client = openai.OpenAiClient.new(model = "o1", api_key = "sk-test");`, with the comment
"The legacy `allowed_roles` option went away with `client<llm>` config blocks; roles are the client's wire concern now."

### 1.3 The `@spec` desugar (the actual dispatch)

Landed in `e8ad36fcf` ("feat(ai): builtin ai/provider packages + @spec desugar for LLM functions", #4352);
`3085fcf5e` then removed legacy Jinja prompts (#4367).

Every LLM function now gets three compiler companions (`crates/baml_compiler2_ast/src/companions.rs:28`):

| Companion | Builder | Body synthesizer | Shape |
|---|---|---|---|
| `Fn$spec` | `companions.rs:106-133` | `lower_expr_body.rs:278-...` (pre-lowered in `lower_cst.rs:369`) | `ai.FunctionSpec<Out> { spec_name, args, prompt_template, toolbox, default_client }` |
| `Fn$render_prompt` | `companions.rs:135-163` | `lower_expr_body.rs:615-631` | `Fn$spec(...).prompt(ai.wire.render_output_format(reflect.type_of<Out>()))` |
| `Fn$parse` | `companions.rs:165-183` | `lower_expr_body.rs:693-...` | network-free `baml.sap.parse<Out>` |
| `Fn$stream` | synthesized at PPIR (`companions.rs:15-16`) | `lower_expr_body.rs:814-840` | `ai.stream.from_spec<Out$stream, Out>(Fn$spec(...), client = client)` |

A **direct call** to a spec-mode LLM function desugars to
`ai.Agent<Out>.new(client = client).run(Fn$spec(p1, p2)).value`
(`crates/baml_compiler2_ast/src/lower_expr_body.rs:729-747`), with `client: ai.Client? = null` injected
as a compiler parameter.

`Fn@spec(args)` postfix sugar rewrites the last path segment to `Fn$spec`
(`crates/baml_compiler2_ast/src/lower_expr_body.rs:3224-3261`).

### 1.4 `client "provider/model"` → native constructor, at compile time

`crates/baml_compiler2_ast/src/lower_cst.rs:744-753`:

```rust
pub(crate) fn spec_client_provider(client: &str) -> Option<(&'static str, &'static str)> {
    match prefix {
        "openai"      => Some(("openai", "OpenAiClient")),
        "anthropic"   => Some(("anthropic", "AnthropicClient")),
        "google"      => Some(("google", "GoogleClient")),
        "claude-code" => Some(("claude_code", "ClaudeCodeClient")),
        _ => None,
    }
}
```

`LlmClientSpec` (`lower_cst.rs:757-768`) has exactly two shapes: `Provider { pkg, class, model }` from the
string, or `Expr(...)` — an arbitrary expression evaluating to `ai.Client`. `resolve_llm_client`
(`lower_cst.rs:773-827`) emits `InvalidLlmClient` for a string with no `/` (line 787), an unknown prefix
(line 795, telling the user to write `openai.OpenAiClient.new(base_url = …)`), and for the removed
unquoted shorthand (line 815-825).

**There is no runtime provider registry and no `PrimitiveClient` path.** `PrimitiveClient`
(`crates/sys_llm/src/baml_std.rs:22-143`) is constructed only in `sys_llm`'s own tests
(`baml_std.rs:369-434`, `specialize_prompt/mod.rs:97-119`, `lib.rs:996+`).

---

## 2. `crates/sys_llm/src/baml_std.rs` — what it actually is

Despite the name, this file exports **no BAML builtin functions**. It is the Rust mirror of the legacy
client-config surface:

- `PrimitiveClient` (`baml_std.rs:22-36`) + `PrimitiveClient::new` (`:39-124`) — name/provider/model/
  default_role/allowed_roles/extra_body/provider_options.
- `is_finish_reason_allowed` (`:125-143`) — the `finish_reason_allow_list`/`deny_list` policy.
- `ProviderOptions` enum (`:154-178`) and `resolve_provider_options` (`:180-206`) — downcasts a
  `BexExternalValue::Instance` by `class_name` (`"baml.prompt.AnthropicOptions"`, …) and `unreachable!`s
  on anything else (`:203-205`).
- `apply_provider_defaults` (`:216-357`) — the runtime default table: `base_url` per provider (`:217-237`),
  `allowed_roles` (`:238-244`), `default_role` with clamping (`:245-272`), `remap_roles`
  (`assistant→model` for Google/Vertex-non-Claude, `:273-288`), and `media_url_handler` per provider/kind
  (`:289-356`).
- `HttpRequest` (`:359-365`) — the plain `{method,url,headers,body}` struct.

The types it names (`PrimitiveClientOptions`, `AnthropicOptions`, `AzureOpenAiOptions`, `BedrockOptions`,
`GoogleAiOptions`, `VertexAiOptions`, `MediaUrlHandler`) are **generated from BAML**:
`crates/baml_builtins2/baml_std/baml/ns_prompt/sys_llm_types.baml`, registered at
`crates/baml_builtins2/src/lib.rs:122` (`builtin!("baml", "ns_prompt/sys_llm_types.baml")`), and re-exported
through `sys_types::generated::owned::prompt` (`baml_std.rs:146-152, 208`).

That `.baml` file's own header (`sys_llm_types.baml:1-4`) already declares the situation:

> "Internal compatibility schemas for the Rust-only `sys_llm` implementation. They are generated into Rust
> data types but are not part of the `ai.Agent` execution architecture. New provider implementations expose
> `ai.Client` values and lower `ai.Prompt` directly."

**These classes are not referenced by any other `.baml` file** (grep of `baml_std/` for `PrimitiveClientOptions`,
`MediaUrlHandler`, `VertexAiOptions`, … finds only `sys_llm_types.baml` itself). They exist solely to
generate Rust structs for dead Rust. → **Delete in phase 2, along with the `builtin!` line.**

### 2.1 The Rust builtins that ARE live in the LLM area

These are declared in `.baml` and implemented in Rust, and they *are* on the hot path:

| BAML decl | Rust impl | Notes |
|---|---|---|
| `baml.prompt.render_output_format(type) -> string` (`baml/ns_prompt/prompt.baml:46-48`) | `sys_ops/src/lib.rs:707-716` → `sys_llm::render_output_format` (`sys_llm/src/lib.rs:67`) | called from BAML by `ai.wire.render_output_format` (`ai/ns_wire/wire.baml:36-38`) |
| `baml.prompt.build_output_format(type) -> OutputFormat` (`prompt.baml:51-53`) | `sys_ops/src/lib.rs:718-729` → `sys_llm::build_output_format_content` (`lib.rs:125`) | opaque handle for the standalone `prompt` tag |
| `baml.prompt.Context.output_format_with(...)` (`prompt.baml:29-41`) | `sys_ops/src/lib.rs:167-207` → `sys_llm::render_output_format_content` (`lib.rs:88`) | 9 render knobs |
| `baml.prompt.get_return_type(string) -> type` (`prompt.baml:61-63`) | `sys_ops/src/lib.rs:731-742` | reflection compat shim |
| `baml.sap.ParseCache.new` (`baml/ns_sap/sap.baml:12-14`) | `sys_ops/src/lib.rs:145-163` → `sys_llm::SapParseCache::new` | wraps `bex_sap::CompiledSapModel` |
| `baml.sap.__parse_final` / `__parse_partial` (`sap.baml:28-41`) | `sys_ops/src/lib.rs:748-791` → `sys_llm::execute_sap_parse_{final,partial}` (`lib.rs:927, 955`) | thin delegates to `bex_sap` |
| `baml.prompt.make_role(string) -> Role` (`prompt.baml:56-58`) | pure BAML — no Rust | |
| `ai.internal.assemble_prompt(parts, values) -> ai.Prompt` (`ai/ns_internal/helpers.baml:27-29`) | `crates/bex_vm/src/package_baml/prompt.rs:240-256` | VM native, not a sys-op |
| `ai.Prompt.text()` / `.messages()` (`ai/spec.baml:23-31`) | `crates/bex_vm/src/package_baml/prompt.rs:212-238` | |

---

## 3. Inventory of the cross-cutting Rust

Sizes: `wc -l` on `crates/sys_llm/src/**` totals **20,391** lines.

| Module | Lines | Live? | Disposition |
|---|---:|---|---|
| `types/output_format.rs` | 3277 | **LIVE** | keep (schema rendering; see §3.6) |
| `types/sap.rs` + `types/mod.rs` | 111 | **LIVE** | keep / relocate |
| `lib.rs` (output-format + SAP entry points, `walk_ty`) `:59-310, 927-980` | ~300 | **LIVE** | keep |
| `build_request/**` (`mod` 1965, `google` 1201, `bedrock` 972, `openai/chat_completions` 943, `anthropic` 844, `openai/responses` 570, `openai/images` 217, `openai/mod` 7) | 6719 | dead | delete |
| `parse_response/**` (`mod` 370, `openai/chat_completions` 603, `anthropic` 452, `google` 451, `openai/responses` 346, `bedrock` 323, `openai/images` 84, `openai/mod` 17) | 2646 | dead | delete |
| `auth_request/**` (`vertex` 1261, `bedrock` 842, `mod` 529) | 2632 | dead | **harvest, then delete** (§5.2) |
| `specialize_prompt/**` (`transformations` 651, `mod` 223) | 874 | dead | port to BAML (§3.1) |
| `resolve_media.rs` | 833 | dead | port to BAML + one new seam (§3.3) |
| `stream_accumulator.rs` | 762 | dead | superseded by `ai.stream` (§3.2) |
| `model_features.rs` | 116 | dead | superseded by per-client BAML fields (§3.4) |
| `provider.rs` | 69 | dead | superseded by `spec_client_provider` + client classes |
| `baml_std.rs` | 429 | dead | delete + delete `sys_llm_types.baml` |
| `lib.rs` (`execute_*`, media/union parse helpers, tests) | ~1620 | dead | delete |

### 3.1 `specialize_prompt/` — six transforms, none of which exist natively

`crates/sys_llm/src/specialize_prompt/mod.rs:18-41` runs, in order:

1. `wrap_simple_as_message(prompt, client.default_role)` — `transformations.rs:12`
2. `promote_media_to_user_when_no_user_message` — `transformations.rs:94`, gated by
   `provider_promotes_media_to_user` (`mod.rs:43-54`: OpenAI family + Anthropic)
3. `merge_adjacent_roles` — `transformations.rs:32`
4. `consolidate_system_prompts(prompt, features)` — `transformations.rs:158`, driven by
   `ModelFeatures::max_one_system_prompt`
5. `validate_and_remap_roles(prompt, client.allowed_roles, client.options.remap_roles)` —
   `transformations.rs:225`; errors with `SpecializePromptError::DisallowedRole` (`mod.rs:56-60`)
6. `filter_metadata(prompt, features)` — `transformations.rs:273`

**Native status: none of these run.** `ai.Prompt.messages()` (`bex_vm/src/package_baml/prompt.rs:225`)
hands the provider the raw `(role, content)` list from `PromptAst::to_messages()`
(`crates/baml_builtins2/src/adt.rs:89-107`). Providers do their own ad-hoc equivalent inline:

- OpenAI: role-less → `"user"`, `"tool"` → `"user"` (`openai/ns_internal/responses.baml:41-49`).
- Anthropic: splits `system` out of the message list (see `crates/baml_tests/tests/structured_prompt_requests.rs:57`
  `anthropic_splits_system_from_prompt_messages`).
- Google: remaps `assistant → "model"` inline (`google/ns_internal/gemini.baml:88-89`, again at `:183`)
  and hoists system instructions out of `contents` (`:76`), so the legacy `remap_roles` default
  (`sys_llm/src/baml_std.rs:273-288`) is covered for this provider. It is *not* generic: a user-written
  client gets no remapping at all.

There is no `allowed_roles` validation anywhere native — by design, per the fixture comment at
`crates/baml_tests/projects/compiles/o1_allowed_roles/o1_clients.baml:2-3`.

### 3.2 `stream_accumulator.rs` — fully superseded

`crates/sys_llm/src/stream_accumulator.rs:63-114` registers per-provider accumulator state in a global
registry; `extract_delta` (`:149-252`) understands only two wire shapes:
OpenAI chat-completions `choices[0].delta.content` (`:154-195`) and Anthropic `content_block_delta`
(`:196-251`). `new_accumulator` explicitly rejects everything else (`:73-87`), *including OpenAI Responses*
(the comment at `:67-71` says the Responses SSE shape "is not yet handled").

Native replacement, and it is strictly better:

- `ai.stream.TurnStream` (`ai/ns_stream/stream.baml:68-239`) — pull-based, folds `TextDelta`/`TurnMeta`/
  `TurnDone` (`:34-52`), backed by `baml.http.SseStream` (`baml/ns_http/http.baml:97-112`).
- `ai.stream.decode_sse_batch` (`stream.baml:26-28`) turns one `SseStream.next()` batch into `SseEvent[]`.
- Each provider ships its own decoder: `_openai_decode_batch` (`openai/ns_internal/responses.baml:255-297`,
  handles `response.output_text.delta` + `response.completed|incomplete|failed`),
  `_google_decode_batch` (`google/ns_internal/gemini.baml:366-427`), and the Anthropic equivalent.
- `ai.stream.Stream<TStream,TFinal>` (`stream.baml:244-290`) is the typed partial-value stream, feeding
  `baml.sap.__parse_partial` / `__parse_final`.
- `ai.stream.from_spec` (`stream.baml:302-350`) is the `Fn$stream` target. **Limitation:** it throws
  `baml.errors.InvalidArgument` when the spec has a non-empty toolbox (`stream.baml:306-310`) — no tool
  loop while streaming.

`stream_accumulator.rs` → **delete outright**; nothing to harvest.

### 3.3 `resolve_media.rs` — the one real functional gap

`crates/sys_llm/src/resolve_media.rs:73-79` `resolve_media()` walks the `PromptAst` and, per media kind,
applies one of four strategies (`ResolveMediaUrls`, `:23-28`): `SendBase64`, `SendUrl`,
`SendUrlAddMimeType`, `SendBase64UnlessGoogleUrl`. It fetches URLs (`:156-258`), reads files (`:259-291`),
parses `data:` URLs (`:316-330`), and infers MIME from headers/magic bytes (`:292-315, 331-...`, via the
`infer` crate). The per-provider strategy table lives in `baml_std.rs:289-356`.

**Native status: media is destroyed before a provider ever sees it.**
`ai.Prompt.messages()` returns `ai.PromptMessage { role: string, content: string }`
(`ai/spec.baml:7-13`), built from `PromptAst::to_messages()` which calls `PromptAstSimple::to_text()`
(`crates/baml_builtins2/src/adt.rs:129-139`); a media part becomes `media.to_string()`, i.e. the *Display
placeholder* `image::url(https://…, loaded=false)` (`crates/baml_builtins2/src/media.rs:209-213, 215-236`).

So today an image interpolated into a `@spec` prompt is silently sent as that debug string. The prompt
assembler *does* keep media structurally up to that point (`bex_vm/src/package_baml/prompt.rs:50-57`
`try_push_special` → `PromptAstSimple::Media`), so the information exists — `ai.Prompt` just has no
accessor for it.

Good news for the port: the ingredients are already native.
`baml.Image/Audio/Video/Pdf` expose `.url()`, `.file()`, `.base64()`, `.mime_type()`
(`baml/ns_media/media.baml` — e.g. `Pdf` at `:2-43`), `baml.http.fetch` (`baml/ns_http/http.baml:127-135`),
and `uint8array.to_base64()` (`baml/uint8array.baml:107`). The one trap:
`MediaValue::base64()` returns `""` for a URL-sourced value that was never resolved
(`crates/baml_builtins2/src/media.rs:134-147`) — native code must fetch and encode itself.

**→ New seam required:** `ai.Prompt` needs structural content, e.g.
`function parts(self) -> (Text | Media)[]` alongside `messages()`. Then `resolve_media`'s whole 833 lines
become ordinary BAML in each provider's `ns_internal`.

### 3.4 `model_features.rs` — capability flags

`crates/sys_llm/src/model_features.rs:10-17` carries exactly two flags: `max_one_system_prompt` and
`allowed_metadata`. Defaults per provider at `:58-87` (OpenAI family = multi-system; Anthropic/Bedrock/
Google/Vertex = single-system), overridden from `options.allowed_role_metadata` at `:90-115`.

**Native expression:** these are not "flags" in the native world — they are *behavior of the provider's
lowering function*. Anthropic's `anthropic_render` already hoists `system` out of the message array by
construction; there is nothing to configure. `allowed_role_metadata` has no native counterpart at all
(`baml.prompt.Role.metadata` exists at `baml/ns_prompt/prompt.baml:10-14` but `make_role` always sets `{}`
(`prompt.baml:56-58`), and `PromptAst::to_messages()` drops metadata entirely
(`crates/baml_builtins2/src/adt.rs:95-107`) — so Anthropic `cache_control` per message is currently
unreachable end-to-end).

**Recommendation:** do not port `ModelFeatures`. Where a real capability decision must be visible to
generic code (e.g. "can this client stream?"), the language already has the right tool: an interface.
`ai.stream.StreamingClient requires ai.Client` (`ai/ns_stream/stream.baml:12-14`) is exactly the
capability mechanism — a client either implements it or `ai.errors.StreamingUnsupported` is thrown
(`ai/ns_errors/errors.baml:113-120`, thrown at `stream.baml:317-319`). Additional capabilities (vision,
tools, JSON mode) should follow the same pattern: **a new interface, not a boolean table.**

### 3.5 `types/output_format.rs` — LIVE, keep in Rust for now

3277 lines. Reached from BAML via `ai.wire.render_output_format` → `baml.prompt.render_output_format` →
`sys_ops/src/lib.rs:715` → `sys_llm::render_output_format` (`lib.rs:67-76`), which builds an
`OutputFormatContent` by walking a `RuntimeTy` and pulling class/enum/alias definitions off
`SysOpContext` (`lib.rs:125-138`, `walk_ty` at `:187-310`), then renders it.

`render_output_format_content` (`lib.rs:88-121`) exposes nine knobs — `prefix`, `or_splitter`,
`enum_value_prefix`, `hoisted_class_prefix`, `always_hoist_enums`, `quote_class_fields`, `hoist_classes`,
`map_style` (`"type_parameters"` vs the default object literal, `:107-112`), `render_null_as` — matching
`baml.prompt.Context.output_format_with` (`baml/ns_prompt/prompt.baml:29-41`).

`is_text_or_image_union` (`types/mod.rs:9`) is also used by the dead `image_generation_mode`
(`lib.rs:465-503`) — check before deleting that helper.

**This is not "SAP".** It is schema-aware *prompting* (the schema-as-text renderer). It is **not** native
in `ai/` and porting it is a large, separate project. Keep it.

### 3.6 `types/sap.rs` — a 45-line wrapper, LIVE

`crates/sys_llm/src/types/sap.rs:5-45` is a thin newtype over `bex_sap::CompiledSapModel`. The actual
parsing lives in `bex_sap` (`sys_llm/src/lib.rs:927-953` and `:955-981` just call
`bex_sap::jsonish::parse` + `TyResolvedRef::coerce` + `to_external::baml_value_to_external`).

The SAP *surface* is already native: `baml.sap.parse<T>` (`baml/ns_sap/sap.baml:44-47`),
`parse_type` (`:51-54`), `ParseCache` (`:7-15`), `NoYield` (`:18-19`). `ai.Agent._parses`
(`ai/runner.baml:99-106`) and `ai.stream.Stream.next/final` (`ai/ns_stream/stream.baml:251-289`) consume it.

**Recommendation:** move `SapParseCache` + the two `execute_sap_parse_*` functions into `bex_sap` or
`sys_ops` and drop `sys_llm` as a dependency edge entirely once `output_format.rs` also moves.

---

## 4. Target architecture — the seams a fully-native provider touches

Reference implementation: `openai/responses.baml` (73 lines, public) + `openai/ns_internal/responses.baml`
(315 lines, everything else).

### 4.1 Seams used by `openai-responses` today

| Seam | Where used | Notes |
|---|---|---|
| `baml.env.get(name)` / `get_or_panic(name)` (`baml/ns_env/env.baml:6, 11`) | `openai/responses.baml:33-52` | credentials resolved **at request time**, never in `new()` — construction stays pure so `$init` can evaluate `client X = …` |
| `baml.http.Request` (`baml/ns_http/http.baml:8-13`) | `openai/ns_internal/responses.baml:159-166` | `{method,url,headers,body}` — same shape as `sys_llm::baml_std::HttpRequest` (`baml_std.rs:359-365`) |
| `baml.http.send` (`http.baml:145-152`) | via `ai.wire.send_as` | |
| `baml.http.fetch_sse` (`http.baml:165-167`) | `openai/ns_internal/responses.baml:300-310` | returns `SseStream` |
| `baml.json.to_json` / `stringify` / `from_string<T>` | `responses.baml:164`, `:262` | envelope classes ignore unknown fields |
| `ai.wire.send_as<T>(req, provider)` (`ai/ns_wire/wire.baml:7-32`) | `responses.baml:196` | send + classify non-2xx + decode; throws `NetworkFailure` / `classify_http` / `ParseFailed` |
| `ai.wire.render_output_format(type)` (`wire.baml:36-38`) | `responses.baml:127` | the `${ctx.output_format}` text |
| `ai.errors.normalize(e)` (`ai/ns_errors/errors.baml:17-23`) | `openai/responses.baml:64-66` | closes the untyped channel at the `ai.Client.invoke` boundary |
| `ai.errors.classify_http` (`errors.baml:124-137`) | inside `send_as` | 429 → `RateLimited`, 408/5xx → `NetworkFailure`, else `InvalidRequest` |
| `ai.Client` interface (`ai/turn.baml:43-46`) | `openai/responses.baml:57-67` | `id()` + `invoke(ModelTurnInput) -> ModelTurn` |
| `ai.stream.StreamingClient` (`ai/ns_stream/stream.baml:12-14`) | `openai/responses.baml:69-73` | |
| `ai.stream.TurnStream.from_sse` + `decode_sse_batch` (`stream.baml:89-106, 26-28`) | `responses.baml:255, 313` | |
| `ai.ModelTurnInput.prompt(output_format)` (`ai/turn.baml:9`) | `responses.baml:126` | the bound prompt thunk |
| `ai.Prompt.messages()` (`ai/spec.baml:29-31`) | `responses.baml:40` | **string-only — the media gap** |
| `ai.Journal.entries()` + `ai.events.*` (`ai/journal.baml`, `ai/ns_events/events.baml`) | `responses.baml:58-116` | journal → wire input items |
| `ai.tools.Toolbox.list()` / `Tool.input_schema` (`ai/ns_tools/tools.baml:10-40`) | `responses.baml:133-147` | schema from `reflect.signature` + `baml.json.schema` (`ai/ns_internal/helpers.baml:5-14`) |
| `ai.content.{Text,Reasoning,ToolUse,StopReason}` (`ai/ns_content/content.baml`) | `responses.baml:170-215` | canonical output |
| `ai.events.Usage` (`ai/ns_events/events.baml`) | `responses.baml:217-227` | |

Reliability composes above the client, in BAML: `ai.Retry` (`ai/ns_clients/clients.baml:49-104`),
`ai.RoundRobin` (`:110-145`), `ai.Fallback` (`:147-190`), gated by `Failure.retry_safety()`
(`ai/ns_errors/errors.baml:10-12`).

### 4.2 What the remaining providers need

Native coverage today (grep of `baml_std/` for endpoints): only three HTTP endpoints exist —
`/responses` (`openai/ns_internal/responses.baml:162`), `/v1/messages`
(`anthropic/ns_internal/messages.baml:241`), `:generateContent` / `:streamGenerateContent?alt=sse`
(`google/ns_internal/gemini.baml:274, 278`) — plus the `claude_code` CLI harness
(`claude_code/ns_internal/cli.baml:277-280`, via `baml.sys.start_process`).

`sys_llm`/engine covered thirteen `LlmProvider` variants (`crates/sys_llm/src/provider.rs:7-58`).
Gap list:

| Legacy provider | Native today | What is needed |
|---|---|---|
| `openai` (chat-completions) | ✗ (only Responses) | **New BAML client**: `openai.ChatCompletionsClient` — `/chat/completions` body + `choices[0].message` parse + `choices[0].delta` SSE decoder. No Rust. |
| `openai-generic`, `ollama`, `openrouter` | ✗ | same chat-completions client with `base_url` override. No Rust. |
| `azure-openai` | ✗ | chat-completions client + `api-key` header + `api-version` query param; URL from `resource_name`+`deployment_id` (validation currently in `baml_base::validate_client_options`, called at `sys_llm/src/baml_std.rs:74-85`). No Rust. |
| `openai-responses` | ✓ | — |
| `anthropic` | ✓ | — |
| `google-ai` (API-key) | ✓ | — |
| `google-ai` + `enterprise` / `GOOGLE_GENAI_USE_VERTEXAI` | ✗ | routing logic at `sys_llm/src/build_request/mod.rs:76-83`, `google_use_enterprise` at `:41-58` — port to BAML `baml.env.get` checks. |
| `vertex-ai` (Gemini) | ✗ | Gemini body reuse + Vertex URL (`{location}-aiplatform.googleapis.com`, placeholders at `build_request/google.rs`) + **OAuth2 token** → needs Rust auth seam. |
| `vertex-ai` + Claude (`rawPredict`) | ✗ | Anthropic body + Vertex URL (`build_request/mod.rs:196-233`) + same token seam. |
| `aws-bedrock` | ✗ | Converse body (`build_request/bedrock.rs`) + **SigV4 signing** → needs Rust auth seam. |
| `ai-gateway-images` | ✗ | image-generation endpoint (`build_request/openai/images.rs`, 217 lines) — low priority. |
| `baml-fallback`, `baml-round-robin` | ✓ | already `ai.Fallback` / `ai.RoundRobin` (`ai/ns_clients/clients.baml:110-190`). |

---

## 5. What Rust legitimately survives

### 5.1 Keep (not auth, but genuinely Rust)

1. **`types/output_format.rs`** (3277 lines) + its `lib.rs` entry points — the `ctx.output_format`
   schema renderer. Reached through `baml.prompt.render_output_format` / `build_output_format` /
   `Context.output_format_with` (`baml/ns_prompt/prompt.baml:46, 51, 29`).
2. **SAP** — `bex_sap` proper, plus the 45-line `SapParseCache` wrapper (`types/sap.rs`) and the two
   `execute_sap_parse_*` delegates (`lib.rs:927, 955`). Surfaced natively already via `baml.sap.*`.
3. **`ai.internal.assemble_prompt` + `ai.Prompt.text()/messages()`** — VM natives in
   `crates/bex_vm/src/package_baml/prompt.rs:212-256`; they own the tagged-template → structural-prompt
   conversion and must stay VM-side.
4. **`baml.http` / `baml.env` / `baml.json` / `baml.sys` / `baml.media` / `reflect`** — the general
   platform seams; nothing LLM-specific.

### 5.2 New Rust seams the remaining providers need — auth only

Per the user's constraint (*auth only*), exactly two families justify new `$rust_io_function`s. Both
already exist as working Rust that can be lifted almost verbatim:

**(a) Google Cloud OAuth2 access token — for `vertex-ai`.**
`crates/sys_llm/src/auth_request/vertex.rs:54-...`. It resolves credentials in the documented order
(`vertex.rs:1-28`: `options.credentials` file path → `options.credentials_content` inline JSON → ADC =
`GOOGLE_APPLICATION_CREDENTIALS` → well-known ADC file → GCE metadata server), then mints a token through
the vendored `forks/google-cloud-auth` (`Cargo.toml:33` `google-cloud-auth`), whose IO is routed through
`RuntimeIo` by `BamlTokenIo`. Signing is pure Rust (`rsa` + `sha2`) so it works on wasm. It also resolves
`project_id` (`vertex.rs:305-327`) and the quota project (`:328-354`).

Proposed BAML surface (bikeshed the names):

```baml
// baml/ns_ai/... or google/ns_internal/
class GcpToken { access_token: string, project_id: string?, quota_project: string?, expires_at_ms: bigint }
function gcp_access_token(
    credentials_path: string?,
    credentials_json: string?,
    scopes: string[],
) -> GcpToken throws root.errors.Io { $rust_io_function }
```

Everything else Vertex needs (URL construction, location resolution from `GOOGLE_CLOUD_LOCATION`,
the `"global"` endpoint rewrite at `vertex.rs:97-113`) is string work → BAML.

**(b) AWS SigV4 request signing — for `aws-bedrock`.**
`crates/sys_llm/src/auth_request/bedrock.rs:117-158` calls `aws_sigv4::sign_request(method, url, headers,
body, credentials, region, "bedrock", now)` from the vendored `forks/aws-bedrock` / `aws-sigv4`, and
resolves credentials/region through `aws_config` over `RuntimeIo` (`bedrock.rs:160-215`, adapter at
`:37-...`). Note `credential_process` shells out natively (`bedrock.rs:34-36`).

Proposed BAML surface:

```baml
class AwsCredentials { access_key_id: string, secret_access_key: string, session_token: string? }
function aws_resolve_credentials(profile: string?) -> AwsCredentials throws root.errors.Io { $rust_io_function }
function aws_resolve_region(profile: string?) -> string throws root.errors.Io { $rust_io_function }
function aws_sigv4_sign(req: baml.http.Request, creds: AwsCredentials, region: string, service: string)
    -> map<string, string> throws root.errors.Io { $rust_io_function }
```

**(c) One non-auth seam is unavoidable: structural prompt content.** See §3.3 — `ai.Prompt` must expose
media, or every multimodal prompt keeps silently degrading to `image::url(…, loaded=false)`. This is a
VM-native accessor next to `messages()` in `crates/bex_vm/src/package_baml/prompt.rs`, not a new subsystem.

**(d) Optional: the WASM playground proxy.** `crates/sys_llm/src/build_request/mod.rs:127-186`
(`BOUNDARY_PROXY_URL` + `baml-original-url` header rewrite, `apply_proxy_rewrite` at `:165-186`) is
`#[cfg(target_arch = "wasm32")]` only. It is pure string manipulation on top of `baml.env.get` → port to
BAML inside `ai.wire.send_as` or a shared helper, do **not** re-add Rust for it. Losing it breaks the
browser playground for LLM calls.

### 5.3 Delete in phase 2

- `crates/sys_llm/src/build_request/**` (6719 lines)
- `crates/sys_llm/src/parse_response/**` (2646 lines)
- `crates/sys_llm/src/auth_request/**` (2632 lines) — *after* harvesting §5.2 (a) and (b)
- `crates/sys_llm/src/specialize_prompt/**` (874 lines)
- `crates/sys_llm/src/resolve_media.rs` (833)
- `crates/sys_llm/src/stream_accumulator.rs` (762)
- `crates/sys_llm/src/model_features.rs` (116), `provider.rs` (69), `baml_std.rs` (429)
- `crates/sys_llm/src/lib.rs:313-925` (`execute_specialize_prompt_from_owned`,
  `execute_build_request*`, `apply_output_request_features`, `enable_openai_*_image_*`,
  `image_generation_mode`, `add_stream_flag_to_request_body`, `execute_validate_finish_reason`,
  `execute_parse_response_from_owned`, `parse_llm_output_for_target` and the media/union helpers) + the
  test module `:995-1461`
- `crates/baml_builtins2/baml_std/baml/ns_prompt/sys_llm_types.baml` (73 lines) and its registration at
  `crates/baml_builtins2/src/lib.rs:122`
- `bex_engine/Cargo.toml:44` (unused dev-dep)
- Dependencies that then fall out of `crates/sys_llm/Cargo.toml`: `aws-bedrock`, `aws-sigv4`,
  `aws-config`, `google-cloud-auth`, `infer`, `url`, `base64`, `async-trait`, `web-time`, `strum`,
  `bex_resource_types`, `bex_vm_types`, `tokio`, the `rustls` crypto features (`Cargo.toml:9-16, 18-52`) —
  **unless** (a)/(b) are harvested into a new `sys_auth` crate, which is the cleaner move: put the two
  vendored SDK forks there and let `sys_llm` shrink to output-format + SAP (or dissolve entirely).

### 5.4 What should be RENAMED

Once only §5.1 remains, `sys_llm` no longer builds LLM requests. Suggested split:

- `sys_output_format` (or fold `output_format.rs` into `sys_ops`) — schema rendering.
- `bex_sap` absorbs `SapParseCache` + `execute_sap_parse_*`.
- `sys_auth` (new) — the Vertex OAuth and Bedrock SigV4 seams plus the two vendored forks.
- `sys_llm` deleted.

---

## 6. Parity risks the migration plan must explicitly own

Ordered by blast radius. Each is a behavior the old engine + `sys_llm` had and native BAML does not.

1. **Media in prompts is silently corrupted.** `PromptAstSimple::to_text()` →
   `MediaValue::Display` (`crates/baml_builtins2/src/adt.rs:133-139`, `crates/baml_builtins2/src/media.rs:209-236`).
   No provider can send an image today. Requires the §5.3(c) seam + per-provider lowering + the four URL
   strategies from `sys_llm/src/resolve_media.rs:23-28` / `baml_std.rs:289-356`.
2. **No chat-completions client at all.** Kills `openai` (legacy), `openai-generic`, `azure-openai`,
   `ollama`, `openrouter` — five of thirteen legacy providers (`sys_llm/src/provider.rs:7-58`).
3. **No Vertex, no Bedrock.** Both are auth-gated (§5.2).
4. **Role normalization is per-provider ad hoc, not a shared pass.** Google remaps `assistant→"model"`
   inline (`google/ns_internal/gemini.baml:88-89, 183`) and OpenAI folds role-less/`tool` into `"user"`
   (`openai/ns_internal/responses.baml:41-49`), but there is no equivalent of the six-step
   `specialize_prompt` pipeline (§3.1) — so a user-authored client, or Vertex-with-Claude, gets nothing.
   Decide whether the pipeline becomes a shared `ai.wire.*` helper or stays duplicated.
5. **`finish_reason_allow_list` / `deny_list` is gone.** `sys_llm/src/baml_std.rs:125-143` +
   `execute_validate_finish_reason` (`lib.rs:521-540`). Native `ai.content.StopReason`
   (`ai/ns_content/content.baml:27-32`) has four variants and no policy hook. If this was a shipped
   feature, it needs a native home (a wrapper client, most naturally).
6. **`allowed_roles` validation is gone — intentionally** (`crates/baml_tests/projects/compiles/o1_allowed_roles/o1_clients.baml:2-3`).
   Document it as a breaking change, not an oversight.
7. **Role metadata (Anthropic `cache_control`) is unreachable end-to-end.** `baml.prompt.Role.metadata`
   exists (`baml/ns_prompt/prompt.baml:10-14`) but `make_role` hardcodes `{}` (`:56-58`) and
   `to_messages()` drops it (`crates/baml_builtins2/src/adt.rs:95-107`).
8. **Image *outputs* are gone.** `apply_output_request_features` (`sys_llm/src/lib.rs:360-383`) injected
   the `image_generation` tool for OpenAI Responses (`:421-463`) and `modalities: ["image","text"]` for
   openai-generic (`:383-419`), driven by `image_generation_mode(return_type)` (`:465-507`); parsing lived
   in `parse_response/openai/images.rs` and `lib.rs:650-925`. The fixture
   `crates/baml_tests/projects/compiles/llm_image_outputs/main.baml` declares `-> image`, `-> image[]`,
   and `-> (image | string)[]` LLM functions against `openai.OpenAiClient` — but it is a **compiles-only**
   project (`projects/compiles/`), so it now proves nothing beyond type-checking: the native client never
   sets `tools: [{type: "image_generation"}]` and `openai_parse` (`openai/ns_internal/responses.baml:170-215`)
   only produces `Text`/`Reasoning`/`ToolUse`. These three functions would fail at runtime.
9. **`request_body` / `headers` / `query_params` passthrough is gone.** `sys_llm/src/build_request/mod.rs:113-126`
   merged user headers and query params onto every request; `extra_body` (`baml_std.rs:106-116`) merged
   arbitrary body keys (temperature, top_p, …). Native clients hardcode their bodies
   (`openai/ns_internal/responses.baml:130-158`) — there is no `temperature` anywhere. This is probably
   the most user-visible regression after media.
10. **Streaming + tools is rejected.** `ai/ns_stream/stream.baml:306-310`.
11. **The WASM playground proxy is gone** (§5.2(d)) — LLM calls from the browser playground will CORS-fail.
12. **`ai.errors.classify_http` never populates `retry_after_ms`** (`ai/ns_errors/errors.baml:126`
    hardcodes `null`), so `ai.Backoff.delay_ms`'s hint path (`ai/ns_clients/clients.baml:27-31`) is dead.
    The `Retry-After` header is available on `baml.http.Response.headers` — a one-line fix.

---

## 7. Notes on documents referenced in the task that do not exist here

- `_plan/llm-desugar-capabilities-plan.md` — **not present** anywhere under
  `/Users/aaron/projects/baml`. The four "approved decisions" it is said to record (Provider client param,
  `//baml:llm_capability` + driver fns, `$` companions, desugar + `LegacyClient` bridge) map onto shipped
  code only partially:
  - "Provider client param" → shipped as `spec_client_provider` (`lower_cst.rs:744-753`) + the injected
    `client: ai.Client? = null` parameter (`lower_expr_body.rs:729-747`). ✓
  - "`$` companions" → shipped: `$spec` / `$render_prompt` / `$parse` / `$stream`
    (`companions.rs:28`, `lower_expr_body.rs:814`). ✓
  - "`//baml:llm_capability` + driver fns" → **never shipped**. The only `//baml:` annotations in
    `baml_std/` are `mut_vm` (95), `vm` (56), `fallible` (33), `may_yield` (30), `mut_self` (17),
    `tagged_string` (1). Capability is expressed as an interface instead
    (`ai.stream.StreamingClient requires ai.Client`, `ai/ns_stream/stream.baml:12`).
  - "desugar + `LegacyClient` bridge" → **never shipped, and no longer needed**: `client<llm>` is a hard
    error (§1.1).
- `crates/baml_builtins2/baml_std/baml/ns_ai/{core,capabilities,providers}/` exist as **empty, untracked
  directories** (`git ls-files` returns nothing for them) — leftovers from that abandoned design. Remove.
- Stale comment to fix while nearby: `baml/ns_env/env.baml:21` still refers to `<Fn>$build_request`, a
  companion that no longer exists.
