# sys_llm → native BAML migration plan

**Status: DRAFT — for review.** Research reports (all claims cited to file:line) live in
`_plan/sys_llm_native/research/{openai-chat,openai-responses-images,anthropic,google-vertex,bedrock,architecture,testing,media}.md`.

## 0. Ground truth (what the research established)

1. **sys_llm's provider stack is dead code.** `execute_build_request_from_owned`,
   `execute_parse_response_from_owned`, `execute_specialize_prompt_from_owned`,
   `execute_validate_finish_reason`, and all of `stream_accumulator`/`auth_request`/
   `resolve_media` have **zero callers** outside sys_llm's own `#[cfg(test)]`
   (~15k of 20.4k lines unreachable). The only live pieces are
   `types/output_format.rs` (schema-aware prompt rendering, called via `sys_ops`)
   and the `types/sap.rs` cache shim. There is no LegacyClient bridge; `client<llm>`
   is a hard compile error. Dispatch is the compile-time shorthand map in
   `crates/baml_compiler2_ast/src/lower_cst.rs:744-753` (`openai`, `anthropic`,
   `google`, `claude-code`) → `ai.FunctionSpec.default_client` → `ai.Agent.run`.
   **So: we port sys_llm's provider knowledge (and the engine's, where sys_llm
   regressed), not its wiring — and phase 2 deletion is mostly free.**

2. **Native clients exist for 3 endpoints** (openai Responses, anthropic Messages,
   google generateContent) and are *ahead* of both Rust impls on tools/journal/
   reasoning/streaming, but behind on: request-body passthrough (no `temperature`
   anywhere), headers/query passthrough, usage fields (cached/reasoning tokens
   hardcoded null), several streaming bugs, and all media.

3. **Missing entirely (native):** the whole chat-completions family (`openai-generic`
   base + `openai` chat + `azure-openai` + `ollama` + `openrouter`), `vertex-ai`,
   `aws-bedrock`, `ai-gateway-images`, and an OpenAI `/v1/images/generations`
   client (net-new — no implementation exists in sys_llm *or* the engine;
   sys_llm's `images.rs` is actually the Vercel AI Gateway builder).

4. **Media**: the VM already has first-class media values (`image`/`audio`/`video`/`pdf`
   over `Arc<MediaValue>`); nothing new needed there. What's missing is plumbing:
   `ai.Prompt.messages()` flattens media to a Display placeholder string (inputs
   destroyed before any client sees them), `ai.content.Block` has no Media variant
   (outputs unrepresentable), and the runner SAP-parses `terminal_text()`
   unconditionally — `-> image` compiles clean and dies at runtime.

5. **Irreducible Rust = auth only**, exactly as hoped:
   - GCP: RS256 JWT signing (PKCS#8 + RSA + SHA-256) inside `forks/google-cloud-auth`.
     Everything else (ADC chain, env/project/location resolution) is portable but
     not worth splitting — keep the one-call token minting in Rust.
   - AWS: SigV4 (SHA-256/HMAC) + credential chain (env/profile/SSO/IMDS) in
     `forks/aws-sigv4` + `forks/aws-config` (already slim hand-forks, **not** the AWS SDK).
   - BAML stdlib has zero crypto, so these cannot be pure BAML.

6. **Test gating exists today, zero new code**: `testset "name" { … }` +
   `baml.toml` `[test] default` profile with `--exclude "::live::"` +
   `--profile live` opt-in; `infisical run -- target/debug/baml-cli test --profile live`
   verified end-to-end against openai/anthropic/google. Native mock-provider
   pattern (`baml.http.Server.bind` + SSE replay) already proven in
   `crates/baml_tests/baml_src/ns_http_server/` and `ns_replay/`.

## 1. Target end-state

```
crates/baml_builtins2/baml_std/
  ai/                    # + content.Media block, runner media branch
                         # structural prompt parts: LANDED on this branch (2026-08-13):
                         #   PromptMessage.parts: (string|Image|Audio|Video|Pdf)[] additive field,
                         #   ai.wire.resolve_media (BAML port of sys_llm resolve_media),
                         #   media-input lowering in all 3 existing clients
  openai/
    responses.baml       # upgraded: passthrough, usage, streaming fixes, image_generation tool
                         # + RENAME openai.OpenAiClient -> openai.ResponsesClient (W3; ~97 files:
                         #   stdlib, lower_cst shorthand map + diagnostics, fixtures, LSP test files,
                         #   snapshot regen; the `openai/` shorthand keeps constructing it)
    chat.baml            # NEW: openai.ChatClient (api.openai.com/v1/chat/completions)
    images.baml          # NEW: openai.ImageClient (/v1/images/generations, gpt-image-1/dall-e-3)
    generic.baml         # NEW: openai.GenericClient (base_url REQUIRED, text-collapse)
    azure.baml           # NEW: AzureOpenAiClient (resource/deployment URL, api-key header)
    ollama.baml          # NEW: OllamaClient (localhost:11434/v1, system role allowed)
    openrouter.baml      # NEW: OpenRouterClient
    ns_internal/         # shared chat-completions build/parse/SSE decode
  anthropic/             # upgraded: 6 bug fixes, passthrough, cache tokens, headers
  google/
    gemini.baml          # upgraded: passthrough, media, generationConfig
    vertex.baml          # NEW: VertexClient (OAuth via sys_auth; gemini + publisher/anthropic rawPredict)
  aws/                   # NEW: BedrockClient (Converse, non-streaming; signed via sys_auth)
  vercel/                # NEW: AiGatewayImageClient (port of sys_llm images.rs)

crates/sys_auth/         # NEW Rust crate — the ONLY provider Rust that survives:
  gcp_access_token(credentials_json: string?, scope: string) -> string   (wraps forks/google-cloud-auth)
  aws_sign(req: baml.http.Request, opts: AwsAuthOpts) -> baml.http.Request  (wraps forks/aws-sigv4 + aws-config)
  aws_resolve_region(opts) -> string?

crates/sys_llm/          # DELETED in phase 2 (output_format.rs + sap.rs move to sys_ops or a small new crate)
forks/aws-bedrock/       # DELETED (pure serde + percent-encoding — expressible in BAML)
```

Compiler touch (small, unavoidable Rust): extend the shorthand map in
`lower_cst.rs:744-753` with `azure-openai/`, `ollama/`, `openrouter/`, `vertex/`,
`bedrock/`, `openai-chat/` (final list TBD), and fix its diagnostic that points
OpenAI-compatible users at the Responses client.

## 1.5 Design principle: typed wire classes, not JSON blobs

Every provider API is modeled as **real BAML classes on both sides of the wire**,
not `map<string, unknown>` construction:

- **Requests**: a typed request class per endpoint in `ns_internal`
  (e.g. `AnthropicMessagesRequest { model: string, max_tokens: int,
  system: SystemBlock[]?, messages: MessageParam[], tools: ToolParam[]?,
  tool_choice: ToolChoice?, stream: bool?, temperature: float?, ... }`) with
  typed content-block unions (`TextBlock | ImageBlock | ToolUseBlock |
  ToolResultBlock | ThinkingBlock`). The existing clients' pattern of building
  `map<string, unknown>` bodies inline (`openai_lower_prompt` returning
  `map<string, unknown>[]`) gets replaced as part of each workstream.
- **Responses**: extend the existing envelope-class pattern (`OaResponse` etc. —
  `from_string` ignores unknown fields) to *complete* typed models of what we
  read, with **optional fields wherever the API may omit them** (the current
  non-optional `OaUsage.int` fields are a known ParseFailed risk).
- **The `json` type (`baml.json.json`, the recursive JSON union) is the escape
  hatch, used exactly twice per client**: (a) the
  `request_body` passthrough option and its deep-merge into the serialized typed
  request (merge happens at the json level after rendering the typed class;
  `null` deletes a key), and (b) genuinely open-ended provider fields
  (e.g. tool `input_schema` / `parametersJsonSchema`, `providerMetadata`
  blobs) where typing would just mirror arbitrary JSON.
- SSE stream events likewise get typed event classes per provider
  (`AnthropicStreamEvent = MessageStart | ContentBlockDelta | ...`), replacing
  string-keyed map poking in `_decode_batch`.

This is also the verification surface: the Fable doc-audit checks the typed
classes field-by-field against the provider's published schema, which is much
stronger than auditing ad-hoc map construction.

## 1.6 Cross-check: pi-ai (badlogic/pi-mono) — same shape, different substrate

Research: `research/pi-ai.md` + `research/pi-agent.md`. pi-ai serves **40
providers with 10 wire adapters**; a provider is a ~15-line config record
(`{id, baseUrl, auth, models, api}`), the agent loop sees one function
`stream(model, context, options)`, and `openai-completions` backs 22 of the 40
providers. Code ratio: ~11.4k lines in adapters vs ~1.2k in provider config —
the quirks didn't vanish, they became ~30 typed `compat` flags per API family.
Our plan is the same shape: the `ns_internal` build/parse/SSE core per API
family = pi's wire adapter; a thin client class per provider = pi's provider
record (in BAML the config record and the `ai.Client` impl are the same class).
Adjustments adopted from the comparison:

- **Wrappers are config, not code.** The chat-completions core takes one typed
  `ChatCompat` record (base_url default, auth style bearer/`api-key`-header,
  api-key env var, all-text collapse, `stream_options` support, max_tokens key
  choice, …) — pi's compat-flags idea, but as typed BAML fields with explicit
  per-client defaults. Three pi smells we deliberately avoid: inferring compat
  by baseUrl substring sniffing, branching on provider *name* strings inside the
  core, and auth-scheme sniffing via `api_key.includes(...)`.
- **Shared journal sanitization** (pi's `transform-messages`, ~220 lines, called
  by every adapter): one `ai` helper every client calls when lowering a journal —
  downgrade foreign-provider reasoning blocks to text on model switch, strip
  signatures, synthesize `"No result provided"` for orphaned tool calls, skip
  errored turns. Subsumes two Anthropic bugs already filed (reasoning-only turn
  → `content: []`; assistant-first prompt). Added as **P0.6**.
- **Errors stay typed.** pi classifies retryability with ~45 regexes over
  provider error *text* (each annotated with the GitHub issue that motivated it —
  honest, and unfinishable). Our typed `ai.errors` taxonomy + per-client error
  mapping is the design answer; the §7 error-taxonomy Fable sweep stays.
- **Image generation as a parallel family is validated** — pi models images as a
  fully parallel API stack (`ImagesApi`/`ImagesProvider`, one-shot not
  streaming) — but shipped no OpenAI images adapter and has **no image-output
  slot in its assistant message type**. Our P0.2 + W7 goes further than pi ever
  did; sys_llm's coercion contract remains the reference.
- **Deliberately not adopted (now):** a models.dev-generated model catalog with
  per-model cost/context-window/capability metadata (pi's is load-bearing for
  clamping + cost accounting). Capability stays interface-shaped
  (`ai.stream.StreamingClient`); a catalog is a good later addition, orthogonal
  to this migration.

**DeepSeek Harness cross-check** (`research/dsh-llm.md`, `research/dsh-architecture.md`):
dsh doesn't implement a provider layer — it *wraps pi-ai* (npm dep, inherits the
40-provider catalog) and implements exactly one wire adapter in-repo
(DeepSeek/OpenAI chat-completions, ~990 LOC, kept as a "twin" to validate the
adapter seam). Corroborations for this plan: (a) dsh **refuses** Bedrock/Vertex/
Azure because its config shape can't express SigV4/ADC/api-version auth — the
auth seam is the part everyone punts on, and `sys_auth` is our differentiator;
(b) its error taxonomy is typed at the seam but regex-over-prose at the pi-ai
edge (upstream flattened the Error) — produce typed errors *at the wire
boundary*, which pure-BAML clients do by construction; (c) wire payloads are
`JSON.parse as WireChunk` with zero runtime validation — our §1.5 typed classes
with real parsing are strictly stronger; (d) reasoning replay state is carried
opaquely on assistant messages and **stripped when the route changes adapter** —
adopt that rule in P0.6 (Anthropic thinking signatures are exactly this);
(e) media is input-only in dsh too (image output and generation absent from all
three systems surveyed) — P0.2/W7 is ahead of the field, with sys_llm as the
only reference implementation.

## 2. Phase 0 — prerequisites (sequential; each is PR-sized)

These unblock everything else and are the only non-auth Rust in the project
(stdlib/VM plumbing, not provider logic).

- **P0.1 Structural prompt parts + metadata.** **PARTS HALF LANDED on this
  branch (2026-08-13, ported from the baml2 checkout):** `ai.PromptMessage`
  gained an additive `parts: PromptPart[]` field (`string | baml.media.*`),
  backed by `PromptAst::to_structured_messages()` in `adt.rs` + part
  materialization in `bex_vm/package_baml/prompt.rs`; `content` stays as the
  readable projection (resolves open question #2: in-place additive field, not
  a new accessor). Also landed alongside: `ai.wire.resolve_media` (BAML port of
  sys_llm's media resolution: file→base64, data-URLs, optional URL fetch,
  extension-based MIME) and media-input lowering in the three existing clients
  (pre-payment on W3/W4/W5). **Still open in P0.1:** per-message `metadata`
  exposure (dropped today in `collect_structured_messages`) — needed for
  Anthropic `cache_control`.
- **P0.2 Media outputs in ai.** Add `ai.content.Media` block variant +
  `ModelTurn.media()`; teach `ai.Agent.run` to branch on `spec.output_type()`
  *before* SAP, porting `parse_llm_output_for_target` semantics exactly
  (`image` = exactly-one-else-error, `image[]`, `image?`, `string|image` ordered
  preference, mixed-output rejection, SAP fallback) with its user-facing error
  messages. Same branch gates `_parses`/repair. SAP itself is NOT taught media
  (separate BEP later).
- **P0.3 Client passthrough convention.** One shared shape every client adopts:
  `request_body: json?` (deep-merged into the serialized typed request as the
  last step; `null` deletes a key — engine semantics), `headers: map<string, string>?`,
  `query_params: map<string, string>?`, plus first-class typed params where the
  provider has them (`temperature`, `max_tokens`/`max_output_tokens`, …) as
  fields on the typed request classes per §1.5. Pure BAML + one shared
  json-merge helper.
- **P0.4 `sys_auth` crate.** Extract the two auth seams above from sys_llm's
  `auth_request/` (~30-line adapters over existing forks). Constraint: signing is
  the *last* mutation of a Bedrock request (signature covers final headers+body).
- **P0.5 Test scaffolding.** New test project under `crates/baml_tests/baml_src/`
  (`test` blocks in `baml_std` never run — CLI collects user package only):
  `[test]` profile in its `baml.toml` (`default` excludes `::live::`; `live`
  profile includes it), `testset "live" with testing.Sequential()` per provider,
  top-level-function test bodies (VM local-boxing bug), mock-provider SSE replay
  helpers. Live model ids: `claude-haiku-4-5`, `gemini-2.5-flash` (docs' ids are stale).
- **P0.6 Shared journal sanitization helper** (§1.6): the `ai` helper all
  clients call when lowering `ai.Journal` → provider messages (foreign-reasoning
  downgrade on model switch, orphaned-tool-call repair, errored-turn skipping).
  Pure BAML; pi's `transform-messages` is the reference behavior.

## 3. Phase 1 — provider parity (the ultracode fan-out)

Eight workstreams, one Opus implementation agent each (≤8 concurrent per your cap),
each followed by an **adversarial Fable verifier** that (a) diffs the
implementation against the provider's *live API docs* (WebFetch), (b) checks every
feature row in the research report's parity matrix is implemented or explicitly
deferred, and (c) tries to construct a request the implementation mishandles.
Confirmed findings go back to the implementer; repeat until the verifier finds
nothing CONFIRMED.

Working rules for every agent (baked into prompts):
- Load the `baml-core` skill; use `target/debug/baml-cli` (never `target/debug/baml`);
  `baml describe` for any stdlib symbol; never guess stdlib.
- Model every request/response/SSE-event as typed BAML classes per §1.5;
  `json` only for the passthrough merge and genuinely open-ended fields.
- **Write the entire client + its tests before the first `baml check`** — batch
  fixes, don't check-per-edit.
- Tests are native BAML: request-shape tests against `baml.http.Server` mocks
  (port sys_llm's ~319 unit assertions where they encode real wire behavior),
  plus a small `::live::` testset run via `infisical run -- … --profile live`.
  Rust tests only where a compiler phase is involved.
- Port from the **engine** where sys_llm regressed (list in §5), from **sys_llm**
  where it's the only reference (media outputs, Responses, gateway images).

| # | Workstream | Contents |
|---|---|---|
| W1 | chat-completions core | `ns_internal` shared build/parse/SSE for the family, parameterized by a typed `ChatCompat` record (§1.6); `OpenAiGenericClient` (base_url required, all-text collapse to plain string) + `OpenAiChatClient`. Net-new: `tool_calls` parse + streaming tool-call assembly into `ai.content.ToolUse` (neither Rust impl had it — required for `ai.Client` parity with the Responses client); `stream_options: {include_usage: true}`. |
| W2 | chat-completions wrappers | Config-only `ChatCompat` instantiations plus the few real overrides: Azure (resource/deployment URL construction, `api-key` header, `AZURE_OPENAI_API_KEY`, max_tokens/max_completion_tokens rules), Ollama (`/v1` base, system allowed), OpenRouter (`OPENROUTER_API_KEY`). Target: each wrapper client reads like pi's 15-line provider record. Depends on W1. |
| W3 | openai Responses upgrade | **Rename `openai.OpenAiClient` → `openai.ResponsesClient`** (mechanical but wide: stdlib + `lower_cst.rs:747` map + its diagnostics + ~90 fixture/test files + snapshot regen; do it as the workstream's first, isolated commit). Passthrough (P0.3), full usage (`cached_tokens`, `reasoning_tokens`, optional-int hardening), streaming: handle `response.failed` as error, stop wrapping status errors as `NetworkFailure`, add `response.created`/`output_text.done`/`function_call_arguments.delta` decode; `image_generation` tool injection driven by output type + `image_generation_call` parse (media out, P0.2); `input_image`/`input_audio`/`input_file` (P0.1) — resolve the sys_llm-vs-engine wire discrepancies (`detail:"auto"`, nested `input_audio`) against live docs. |
| W4 | anthropic upgrade | Fix the 6 confirmed bugs (mid-stream `error` event dropped; all stream HTTP errors → retry-safe `NetworkFailure`; unknown stop reasons → `Complete`; reasoning-only turn → `content: []`; assistant-first w/o system invalid; cached tokens null). Passthrough + `anthropic-beta`/browser-access headers; `cache_control` via P0.1 metadata; image/document blocks; streaming decode for `content_block_start`/`input_json_delta` (within existing StreamEvent — tool-call *deltas* still buffer to TurnDone, see §6); keep body builder factored from URL/headers (Vertex rawPredict reuses it). |
| W5 | google + vertex | Gemini: passthrough (`generationConfig`, `safetySettings`), `inlineData`/`fileData` media in+out, usage metadata. New `VertexClient`: OAuth bearer via `sys_auth.gcp_access_token`, project/location resolution (env + credential-file), publisher routing (`google` generateContent vs `anthropic` rawPredict — decide trigger: model-prefix like sys_llm), express-mode API key, `x-goog-user-project`. Streaming for both (`:streamGenerateContent?alt=sse` — native gemini already does this; sys_llm never could). |
| W6 | bedrock | `BedrockClient`: Converse request body + response parse in pure BAML (already hand-built JSON in sys_llm — ports losslessly), signed via `sys_auth.aws_sign` as the final step; region/endpoint resolution; percent-encoded model-id path (ARNs/inference profiles); `cache_read_input_tokens` (free fix); cachePoint support (engine had it, sys_llm dropped it). Non-streaming only (§6). Net-new optional: `toolConfig` from toolbox (neither impl had it) — include, it's what `ai.Client` means. |
| W7 | images | `OpenAiImageClient` — **net-new, verify against live OpenAI docs**: `/v1/images/generations`, `gpt-image-1`/`dall-e-3`, request (`prompt`,`size`,`quality`,`n`,`background`,`output_format`/`response_format`), parse `data[].b64_json` (+`revised_prompt`, usage) → `baml.media.Image.from_base64` → `ai.content.Media`. Plus `AiGatewayImageClient` port (exact headers `ai-gateway-protocol-version: 0.0.1` etc.; fix the hardcoded `image/png` mime bug). Depends on P0.2. |
| W8 | media plumbing + cross-provider media tests | The BAML side of P0.1/P0.2 consumption: per-provider media lowering (port `resolve_media` URL strategies as a shared `ai` helper with per-provider defaults: base64-inline vs URL-passthrough; MIME from data-URL/extension — **no byte sniffing initially**), `-> image` e2e fixtures (`compiles/llm_image_outputs` currently type-checks but dies at runtime), media round-trip live tests. |

Sequencing: P0.1–P0.5 first (P0.4 can parallel P0.1–3). Then W1, W3–W7 in
parallel (6 agents), W2+W8 as W1/P0 land. Fable verifiers run per-workstream on
completion. Finish with one integration pass: full `baml test` (mock) green, then
`infisical run … --profile live` across all providers with available keys
(note: AWS keys in Infisical are empty — Bedrock live needs a local
`AWS_PROFILE=boundaryml-dev` SSO session; I'll run it if a session exists,
otherwise mark Bedrock live-unverified).

## 4. Phase 2 — deletion (after phase 1 is green + verified)

1. Move `types/output_format.rs` (+ `types/sap.rs` shim) out of `sys_llm` — into
   `sys_ops` (its only caller) or a small `baml_output_format` crate.
2. Delete `crates/sys_llm` entirely; drop the dep from `sys_ops`, `bex_cache`,
   `bex_engine` (already stale there); delete `forks/aws-bedrock`.
3. `forks/google-cloud-auth`, `forks/aws-sigv4`, `forks/aws-config` survive as
   deps of `sys_auth` only.
4. Cleanup: `baml/ns_prompt/sys_llm_types.baml` (self-declared dead), stale
   `<Fn>$build_request` mention in `baml/ns_env/env.baml:21`, empty
   `baml_std/baml/ns_ai/{core,capabilities,providers}/` dirs.
5. Fable sweep: grep-audit that no `.baml`/Rust references remain; full CI.

## 5. Divergences to resolve (my recommendation in bold)

| Issue | sys_llm | engine | plan |
|---|---|---|---|
| Ollama base_url | `:11434` (404s) | `:11434/v1` | **engine** |
| openai-generic base_url unset | silently api.openai.com | hard error | **engine** (hard error) |
| Ollama system role | forbidden | allowed | **engine** |
| Azure max_tokens | always injected | skip if max_completion_tokens; `null` removes | **engine** |
| Anthropic max_tokens default | 8192 | 4096 | **8192** (sys_llm's rationale; modern models) |
| Role-less content default role | `system` | — | **`user`** (matches shipped native clients) |
| Message metadata placement (openai) | on message | on content part | **engine** (content part) |
| google-ai `GOOGLE_API_KEY` fallback | missing | present | **engine** (native already has it) |
| Vertex publisher=anthropic trigger | model prefix `claude*` | `anthropic_version` set | **model prefix** (no legacy options natively) |
| multi-system-message handling | demote 2nd+ to user | — | **google: merge into systemInstruction; others: demote** (match engine UX) |

## 6. Explicitly deferred (each needs its own design, none regress current behavior)

- **Streaming tool-call deltas**: `ai.stream.StreamEvent = TextDelta|TurnMeta|TurnDone`
  can't carry them; buffer tool calls until `TurnDone` (native Responses client
  already behaves this way). Extending StreamEvent is a stdlib BEP.
- **Bedrock ConverseStream**: AWS binary eventstream, needs a Rust decoder;
  sys_llm never streamed Bedrock either — zero regression. `StreamingUnsupported` error.
- **SAP coercing media** (media inside structured outputs): separate BEP.
- **fetch_sse/request timeouts**: engine's 4-way timeout config; needs a
  `baml.http` extension — tracked, not in this migration.
- **WASM playground proxy** (`BOUNDARY_PROXY_URL` rewrite) + Anthropic
  browser-access header: the native path never had it; port only the Anthropic
  header (trivial), proxy rewrite is its own task.
- Legacy `/completions` mode, `client_response_type`, o1 special-casing,
  IRSA/AssumeRole for AWS: dropped knowingly (document in changelog).

## 7. Verification protocol (the adversarial layer)

Per workstream, a Fable agent with the research report + live provider docs:
1. **API-contract audit**: every request field, header, auth mechanism, response
   field, SSE event type in the provider's current docs → implemented, deferred
   (listed in §6), or MISSING (finding).
2. **Parity audit**: every row of the research matrix → same trichotomy.
3. **Hostile inputs**: empty prompt, assistant-first, reasoning-only turns,
   4xx/5xx + malformed SSE via mock server, unicode/huge payloads.
4. Findings verified CONFIRMED before going back to the implementer; loop until dry.

Final gate: full mock suite in CI (no secrets), live profile run locally via
Infisical, and a cross-provider Fable sweep comparing the four clients' error
taxonomy usage (`ai.errors.normalize` consistency — no retry-unsafe error may map
to a retry-safe class, the Anthropic streaming bug pattern).

## 8. Appendix: model catalog design (follow-up project, not this migration)

If/when we do it, the shape that fits BAML (informed by what is and isn't
load-bearing in pi's catalog):

- **Data = generated BAML, checked in.** A script (models.dev API → quirk
  corrections → emit) generates `ai/ns_models/*.generated.baml`: a typed
  `ai.models.ModelInfo` class and per-provider constructor functions returning
  `map<string, ModelInfo>`. Committed and regenerated by command (repo snapshot
  culture), not fetched at runtime — builtins stay io-free at `$init`. Scope it
  to providers we ship clients for, first-party lists only (openai, anthropic,
  google, bedrock, vertex), not pi's 40-aggregator sprawl — their generator
  needed ~2,000 lines of quirk-correction tables; a narrow schema and narrow
  provider set is the defense.
- **Schema: only fields with a consumer.** `ModelInfo { id, provider,
  api: ApiFamily, context_window: int?, max_output_tokens: int?,
  cost: Cost? (per-Mtok in/out/cache_read/cache_write),
  caps: { tool_call: bool, reasoning: bool, image_input: bool,
  image_output: bool, caching: bool } }`. Every field must have a named
  consumer at introduction time or it's cut (pi lesson: three of their compat
  flags are documented as auto-detected but hardcoded false — dead config).
- **Advisory, never gating.** `ai.models.lookup(id) -> ModelInfo?` returns null
  for unknown ids and everything keeps working — a day-one model must run
  before the catalog knows it. pi *filters out* non-tool-calling models
  entirely; we warn, not block.
- **The load-bearing consumers, in priority order:**
  1. **Cost accounting**: `run`-level cost from summed `ai.events.Usage` ×
     `ModelInfo.cost` — the single most-requested observability feature and
     pi's main use.
  2. **Per-model `max_tokens` default** for Anthropic/Bedrock (replaces the
     blanket 8192-vs-4096 decision in §5 with the real per-model ceiling).
  3. **Capability preflight**: `-> image` with a non-`image_output` model, or
     `toolbox` with a non-`tool_call` model → typed `ai.errors` failure at
     call-time instead of a provider 400 mid-run.
  4. **Editor surface**: model-id autocomplete/diagnostics for client
     shorthands in LSP/playground (catalog as compiler-readable data is free
     once it's a `.baml` file).
- **User extension is just BAML.** pi needs `~/.pi/models.json` + TypeBox
  schema + hot-reload because their users configure a compiled app; our users
  write code — constructing a `ModelInfo` (or ignoring the catalog entirely
  and passing explicit client params) is the extension mechanism. No registry
  mutation API.
- **One open risk to measure first**: compile-time cost of a large const table
  in `baml_std` (cold-compile perf is a defended budget — 0.81s). If a few
  hundred `ModelInfo` literals measurably regress it, fall back to one compact
  generated JSON string const + lazy `baml.json.parse` in `lookup`.

## 9. Open questions for you

1. **Shorthand prefixes**: which new ones to register, and does `"openai/gpt-4o"`
   stay Responses-only (new `openai-chat/` prefix for chat)? My default: keep
   `openai/` = Responses, add `openai-chat/`, `azure/`, `ollama/`, `openrouter/`,
   `vertex/`, `bedrock/`.
2. ~~**P0.1 API shape**~~ RESOLVED: the landed diff extends `PromptMessage` in
   place with an additive `parts` field (keeping `content`), so there is no
   dual accessor and no breakage.
3. **Anthropic 8192 default** ok? (row in §5).
4. **`vercel/` package name** for the gateway image client, or park it under `openai/`?
5. Bedrock live testing: is a local SSO session (`AWS_PROFILE=boundaryml-dev`)
   acceptable as the verification path, given Infisical's AWS keys are empty?
