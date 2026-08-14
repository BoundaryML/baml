# DeepSeek Harness (dsh) — LLM provider layer

Source surveyed: `deepseek-harness` @ `0.1.0-rc.5` (monorepo checkout, `node_modules` **not** installed —
pi-ai internals were not readable, only its type-level and API usage from dsh's side).
All paths below are relative to the repo root.

**Headline for the comparison**: dsh does not have a provider layer in the pi-ai sense at all.
It has a *seam* (`ctx.llm`, ~950 LOC) plus exactly **two** production adapters: one hand-written
DeepSeek chat-completions adapter (~990 LOC total), and one adapter that **wraps pi-ai itself**
(`@earendil-works/pi-ai ^0.82.1`, i.e. the same library family the parent comparison already mapped).
Everything multi-provider — 10 wire protocols, 40 provider records, models.dev catalog, compat flags —
is *delegated to pi-ai*, and dsh deliberately re-narrows it (3 nameable protocols, 2 compat switches,
api-key auth only).

---

## 0. Package map

`packages/llm/README.md:1-20` names the family:

| Package | Role | ctx key | LOC (src) |
|---|---|---|---|
| `packages/llm/llm` | seam: service, message/content vocabulary, StreamChunk, assembler, retry-policy schema, error taxonomy | `ctx.llm` | 2244 |
| `packages/llm/llm-deepseek` | direct-`fetch` DeepSeek chat-completions adapter | registers on `ctx.llm` | 990 |
| `packages/llm/llm-pi-ai` | pi-ai-backed multi-provider adapter | registers on `ctx.llm` | 2091 |
| `packages/llm/llm-retry` | retry *executor* (listens `agent/request-error`) | — | 494 |
| `packages/llm/token-meter` | replay-aware token measurement | `ctx.tokenMeter` | 872 |
| `packages/credentials/credentials` | credential-**reference** seam | `ctx.credentials` | 214 |
| `packages/credentials/credentials-local` | env + `~/.dsh/.credentials.yaml` + `.env` layered provider | — | 527 |
| `packages/test-support/llm-replay` | third (test-only) `LlmAdapter`: replays recorded `assistant/chunk` logs | — | 856 |

There is **no** `packages/api` LLM content (`packages/api/gateway`, `packages/api/remotes` are the
host↔client RPC transport), and `packages/typert` is an unrelated codegen/RPC-descriptor system
(`docs/subsystems/typert.md:1-10`) — **not** a runtime type system for wire payloads.

The design rationale is an explicit Agent Note: *"Two LLM adapters as a design-verification twin"*
(`.agents/notes/implemented/architecture/2026-06-13-twin-llm-adapters.md:13-18`) — ship two adapters
built on **different internals** so that "anything the StreamChunk vocabulary cannot express for BOTH
implementations is a core-vocabulary bug".

---

## 1. Provider/adapter split

### How many wire-protocol implementations exist *in dsh*

**One.** `packages/llm/llm-deepseek/src/` is the only place dsh speaks HTTP to a model endpoint itself:

- request body construction: `packages/llm/llm-deepseek/src/serialize.ts:151-187`
- SSE framing (delegated to `eventsource-parser`): `packages/llm/llm-deepseek/src/sse.ts:28-40`
- SSE→StreamChunk translation: `packages/llm/llm-deepseek/src/translate.ts:86-185`
- `fetch` + headers + error mapping: `packages/llm/llm-deepseek/src/adapter.ts:271-345`

It targets OpenAI-compatible `POST {baseURL}/chat/completions` with DeepSeek-specific extensions
(`thinking`, `reasoning_effort`, `reasoning_content` passback) — `packages/llm/llm-deepseek/src/types.ts:12-30`.

There is **no** OpenAI Responses, Anthropic Messages, Gemini, or Bedrock implementation written in dsh.

### The other three reach the wire through pi-ai

`packages/llm/llm-pi-ai/src/provider.ts:47-51` is the entire protocol table:

```ts
const PROTOCOLS: Readonly<Record<string, () => ProviderStreams>> = {
  'openai-completions': openAICompletionsApi,
  'openai-responses': openAIResponsesApi,
  'anthropic-messages': anthropicMessagesApi,
}
```

and the doc comment above it (`provider.ts:29-46`) explains the deliberate narrowing:
Bedrock (SigV4+region), Vertex (project/location/ADC), Azure (api-version), and Codex (OAuth) are
**refused for hand-declared routes** because dsh's config shape cannot express their auth. Test:
`packages/llm/llm-pi-ai/tests/catalog.spec.ts:314-325` asserts `supportedProtocols()` excludes
`bedrock-converse-stream`, `google-vertex`, `azure-openai-responses`, `openai-codex-responses`.

Crucially, a *catalog* route (one pi-ai already ships) **reuses pi-ai's own Provider object**
rather than rebuilding it (`provider.ts:144-159`, `buildProvider` at `provider.ts:167-192`), so
catalog routes still reach every pi-ai protocol including Bedrock. Only an explicit `api:` override
is limited to the 3-entry table.

### Is a provider a config record, a plugin, or a class?

**All three, at different layers:**

1. **A class** — `LlmAdapter` abstract base, `packages/llm/llm/src/index.ts:180-233`. One required
   method: `stream(options): AsyncIterable<StreamChunk>`. Optional `providerInfo`,
   `providerRetryPolicy`, `listModels`, `resolveModel`.
2. **A Cordis plugin** — each adapter package exports `name`, `inject`, `Config` (schemastery schema),
   and `apply(ctx, config)`. See `packages/llm/llm-deepseek/src/index.ts:41-42,200` and
   `packages/llm/llm-pi-ai/src/index.ts:84-85,150`.
3. **A config record** — inside the pi-ai plugin, a "provider route" is a key in a `providers` dict
   (`packages/llm/llm-pi-ai/src/config.ts:171-179`). This is the pi-ai-style
   provider-as-config-record, one level down.

`LlmRuntime` (the service) is a registry keyed by **route string** →
`{ adapter, providerInfo, retryPolicy }` (`packages/llm/llm/src/index.ts:284-291, 941-945`).
Registration is all-or-nothing and atomically replaceable:
`registerAdapter(providers: string[], adapter)` → `AdapterRegistrationHandle` with `.replace()`
(`index.ts:338-367`, handle contract at `index.ts:239-257`). Duplicate route → `DUPLICATE_ADAPTER`
(`index.ts:380`).

### Exact registration/config shape a user writes

**Composition file** (`cordis.yml`), e.g. `examples/headless-agent/cordis.yml:20-32`:

```yaml
- id: llm-deepseek
  name: '@deepseek-ai/dsh-llm-deepseek'
  config:
    thinking: enabled
    reasoningEffort: max
    models:
      - id: deepseek-v4-pro
        contextWindow: 128000
      - id: deepseek-v4-flash
        contextWindow: 128000
```

That plugin owns exactly one route, hardcoded: `const PROVIDER = 'deepseek-official'`
(`packages/llm/llm-deepseek/src/index.ts:47`), default catalog two models
(`index.ts:49-52`), config interface at `index.ts:62-81`, schemastery schema `index.ts:91-101`.

**The pi-ai plugin's shape** is the interesting one — the module JSDoc is a worked example,
`packages/llm/llm-pi-ai/src/index.ts:12-53`:

```yaml
- id: llm
  name: '@deepseek-ai/dsh-llm-pi-ai'
  config:
    providers:
      openai:                       # catalog route: everything but the key comes from pi-ai
        apiKeyEnv: OPENAI_API_KEY
        retryPolicy: { mode: normal, maxRetries: 2 }
      anthropic:                    # catalog route, catalog narrowed + one capacity corrected
        apiKeyEnv: ANTHROPIC_API_KEY
        models:
          - id: claude-sonnet-4-5
            contextWindow: 200000
      acme-gateway:                 # hand-declared route: pi-ai ships nothing under this key
        displayName: Acme Gateway
        apiKeyEnv: ACME_GATEWAY_API_KEY
        api: openai-completions
        baseURL: https://gateway.acme.example/v1
        compat:
          thinkingFormat: deepseek
        models:
          - id: acme-think
            name: Acme Think
            contextWindow: 262144
            maxTokens: 32768
            reasoningEfforts:       # key = selectable level, value = wire spelling
              off:
              high: high
              max: ultra
```

Full profile type: `packages/llm/llm-pi-ai/src/config.ts:65-141` (23 fields); schema
`config.ts:232-257`. Per-model profile: `packages/llm/llm-pi-ai/src/catalog.ts:202-238`.
`modelOverrides` (customize one catalog model, keep the rest) is `catalog.ts:240-247`.

The *same* shape is also a **hot-reloaded user-settings section**: `installSettingsSection(ctx, NS, Config, ...)`
at `llm-deepseek/src/index.ts:270-275` and `llm-pi-ai/src/index.ts:278-311`. A settings edit reaches
the **next request** without restart, because connection facts are re-read per operation through a
thunk (`llm-deepseek/src/adapter.ts:74-86`, `llm-pi-ai/src/adapter.ts:199-206`) — and an in-flight
stream keeps the snapshot it started with (`llm-pi-ai/src/adapter.ts:1-21` explains why the Models
collection is rebuilt rather than mutated).

### Provider *directory* (a dsh-specific concept)

Beyond registered routes, the seam has a second registry: `registerConfigurableProviders()`
(`packages/llm/llm/src/index.ts:431-484`), listing routes a plugin *could* activate through
configuration, with a settings address (`LlmConfigurableProvider`, `types.ts:166-187`:
`provider`, `displayName`, `settingsNs`, `settingsPath[]`, `declared?`). The pi-ai plugin publishes
the **entire installed pi-ai catalog** here at mount time (`llm-pi-ai/src/index.ts:120-147`), filtered
to providers that offer api-key auth (`catalogProviderTakesApiKey`, `catalog.ts:160-162`) —
OAuth-only providers like `openai-codex` are withheld because dsh holds no OAuth store
(rationale `catalog.ts:144-159`, test `tests/catalog.spec.ts:940-952`).

This is what powers the web "Models" page: `packages/client/ui-settings-models/`, joined against
credentials in `packages/client/ui-settings-models/src/client/store.ts:150-205`, readiness at
`store.ts:189-205`.

---

## 2. Model catalog

### Seam-level metadata is deliberately thin

`LlmModelInfo` (`packages/llm/llm/src/types.ts:233-244`): `provider`, `id`, `name`, `description?`,
`inputModalities?`. `LlmResolvedModelInfo` (`types.ts:274-281`) adds `context?: {contextWindow}`,
`defaultMaxTokens?`, `reasoning?: {efforts[], defaultEffort?}`.

**There is no cost/pricing field anywhere in the seam.** pi-ai's `ModelCost` is explicitly zeroed:

```ts
// packages/llm/llm-pi-ai/src/catalog.ts:27-32
/**
 * Pricing for a model the installed catalog does not describe. The harness
 * never reads pi-ai's cost metadata — `replay.ts` zeroes it and no consumer
 * reports spend — so this is the absence of a fact, not a configurable rate.
 */
const NO_COST: ModelCost = { input: 0, output: 0, cacheRead: 0, cacheWrite: 0 }
```

and `replay.ts:47-56` constructs `emptyPiUsage()` with zero cost. **dsh reports no spend at all.**

Capability flags are limited to: `inputModalities` (`text` | `image` only —
`ModelModalityMap`, `types.ts:152-158`), `contextWindow`, `defaultMaxTokens`, and a
**reasoning-effort list** (`LlmReasoningEffortInfo`, `types.ts:253-271`). No tool-calling flag, no
structured-output flag, no cache-control flag, no vision-output flag.

Catalog membership is **advisory, never a whitelist** — stated three times:
`types.ts:232`, `index.ts:200-208`, `docs/subsystems/llm-streaming.md:629`.

### Where does metadata come from?

Four sources, no generator:

1. **Hand-written constants in the DeepSeek plugin**: `DEFAULT_MODELS` = V4-Flash / V4-Pro,
   `packages/llm/llm-deepseek/src/index.ts:49-52`; `DEFAULT_CONTEXT_WINDOW = 1_000_000`,
   `DEFAULT_MAX_TOKENS = 256_000` at `llm-deepseek/src/adapter.ts:89-93`. Reasoning efforts are a
   3-entry const array `off/high/max` (`adapter.ts:98-105`).
2. **The installed pi-ai catalog** — imported at runtime from
   `@earendil-works/pi-ai/providers/all` (`catalog.ts:15-16`): `builtinProviders()`,
   `getBuiltinProviders()`, `getBuiltinModels(provider)`. dsh keeps no copy and runs **no
   generation step**; the catalog is whatever the pinned pi-ai version ships (`^0.82.1`,
   `packages/llm/llm-pi-ai/package.json` deps). Catalog defaults are merged *under* config entries
   field by field in `resolveRouteModels()` (`catalog.ts:446-546`), with a **spread-never-enumerate**
   rule so unknown pi-ai `Model` fields survive (`catalog.ts:521-527`).
3. **User configuration** — a `models` list replaces the catalog; `modelOverrides` patches it
   (`catalog.ts:446-490`).
4. **Live endpoint interrogation** — `registerModelDiscovery` / `discoverModels`
   (`packages/llm/llm/src/index.ts:504-559`) and its one implementation
   `packages/llm/llm-pi-ai/src/discovery.ts:195-284`: `GET {baseURL}/models`, bearer auth,
   4 MB bounded read (`discovery.ts:50, 96-131`), tolerant field extraction
   (`context_window|context_length`, `max_output_tokens|max_tokens`, `discovery.ts:52-62, 138-162`).
   Only `openai-completions` and `openai-responses` are listable (`discovery.ts:38-41`).
   A catalog route short-circuits with **no network call** (`discovery.ts:200-211`).
   Discovery results are **never stored** — they are candidates a UI offers for adoption
   (`discovery.ts:11-15`).

### Drift gates instead of codegen

Where pi-ai's vocabulary could drift, dsh compiles a `Record<K, true>` keyed by pi-ai's union so a
pi-ai upgrade **fails compilation** naming the drifted key:
- modalities: `catalog.ts:36-48`
- thinking levels (`off|minimal|low|medium|high|xhigh|max`): `catalog.ts:63-80`
- thinking formats (`openai|deepseek|openrouter|together|zai|qwen|string-thinking|ant-ling`,
  with `chat-template`/`qwen-chat-template` explicitly withheld): `catalog.ts:82-112`

### Who consumes it at runtime

- Agent loop: `prepareCall()` before each step (`packages/core/agent-loop/src/agent.ts:449`),
  freezing the config + registration + `context` for the dispatch (`packages/llm/llm/src/index.ts:779-814`).
- Compaction threshold: `ctx.llm.resolveModelInfo(...).context` at
  `packages/compaction/compaction-basic/src/index.ts:293`.
- Host RPC catalog for UI selectors: `packages/host/apiproxy/src/api-proxy.ts:332-337, 3372-3407`.
- Image admission preflight: `inputModalities` gating, `packages/llm/llm-pi-ai/src/adapter.ts:302-309`.

---

## 3. Request/response typing

### DeepSeek adapter: fully hand-written TypeScript interfaces, no runtime validation

`packages/llm/llm-deepseek/src/types.ts` is a 152-line "types only" module (header at `:1-10` names
the official docs as source of truth). It types **both directions**:

- request: `WireRequest` (`:13-30`), `WireMessage` union discriminated on `role` (`:52-56`),
  `WireSystemMessage`/`WireUserMessage`/`WireAssistantMessage`/`WireToolMessage` (`:33-73`),
  `WireToolCall` (`:76-80`), `WireTool` (`:83-90`)
- streaming response: `WireChunk` (`:93-97`), `WireChoice` (`:100-103`), `WireDelta` (`:106-116`),
  `WireToolCallDelta` (`:119-131`), `WireUsage` (`:140-147`), `WireError` (`:150-152`)

So **SSE events are typed** — as a `WireChunk` interface whose fields are all optional, decoded with
a bare `JSON.parse(payload) as WireChunk` (`translate.ts:120-125`). There is **no zod/schemastery
validation of provider responses**; malformed JSON → `MALFORMED_RESPONSE`, but a
structurally-wrong-but-valid JSON chunk is read optimistically with `typeof` guards
(`translate.ts:132-174`).

Contrast: **configuration** *is* runtime-validated with schemastery (`z.object`, `z.union`,
`z.dict`, `.default()`, `.required()`) — `llm-deepseek/src/index.ts:83-101`,
`llm-pi-ai/src/config.ts:181-257`, `llm/src/retry-policy.ts:81-103`. dsh validates what users write,
not what providers send.

### pi-ai adapter: types borrowed from pi-ai, translated at the boundary

`packages/llm/llm-pi-ai/src/stream.ts:124-208` switches over pi-ai's `AssistantMessageEvent` closed
union with a comment that a new pi-ai event type should break the exhaustiveness check
(`stream.ts:202-204`). Context conversion is `packages/llm/llm-pi-ai/src/context.ts:139-189`.
pi-ai's `TSchema` (TypeBox) is assigned directly from dsh's JSON-Schema `ToolSchema.parameters`
because "TypeBox is structurally JSON Schema" (`context.ts:71-74`).

### `packages/typert` is not involved

Typert is the host↔client remote-method-call codegen (`docs/subsystems/typert.md`,
`packages/typert/protocol/src/types.ts`). It generates invocation descriptors and strict/SRC codecs
for **dsh's own RPC**, not for provider wire payloads. No LLM package depends on it.

### Branded nominal ids

`packages/llm/llm/src/brand.ts`: `MessageId`, `CallId`, `ProviderRequestId`, `ReasoningEffortId`
(all `Branded<'X'>` from a zero-dep `dsh-brand` package). Brand constructors do **no validation**
(`brand.ts:38-40` etc.). `CredentialRef` is branded *with* validation
(`packages/credentials/credentials/src/index.ts:16, 23-28`: must match `/^[A-Za-z_][A-Za-z0-9_]*$/`).

---

## 4. Streaming model

### The unified event vocabulary

`packages/llm/llm/src/types.ts:291-303` — a **closed** 7-variant discriminated union:

```ts
type StreamChunk =
  | { type: 'block-start';      index: number; blockType: ContentBlockType }
  | { type: 'text-delta';       index: number; text: string }
  | { type: 'reasoning-delta';  index: number; text: string }
  | { type: 'tool-call-delta';  index: number; id: CallId; name?: string; argumentsDelta: string }
  | { type: 'block-end';        index: number; block: ContentBlock }
  | { type: 'usage';            usage: TokenUsage }
  | { type: 'finish';           reason: FinishReason; replayState?: unknown }
```

Content blocks (`ContentBlockMap`, `types.ts:99-105`) are **merge-extensible** via TS declaration
merging: `text | reasoning | image | tool-call | tool-result`. `FinishReasonMap`
(`types.ts:116-122`) likewise: `stop | tool-calls | max-tokens | aborted | error`.

The adapter contract is written down in `docs/subsystems/llm-streaming.md:204-217`:
usage **before** finish and nothing after; tool-call `arguments` stay **raw JSON strings**
end-to-end; two sanctioned error paths (throw *or* terminal error finish); one adapter call = one
provider attempt (library retries disabled); provider stalls bounded by a per-read idle watchdog;
one canonical `CONTEXT_WINDOW_EXCEEDED` code; empty completion is a retryable error; every request
carries `attributionHeaders()`; replay state is adapter-owned.

`BlockAssembler` (`packages/llm/llm/src/assembler.ts`, public shape at
`docs/subsystems/llm-streaming.md:266-311`) is the one shared fold back into `ContentBlock[]` +
usage + finish + replayState; tolerant of delta-only protocols; deltas after `block-end` are ignored.

### Partial tool-call handling

- **DeepSeek**: fragments keyed by wire `tool_calls[].index`, mapped into a harness block index
  (`translate.ts:152-170`); `id`/`name` arrive only on the first fragment and are latched
  (`translate.ts:159-160`); `argumentsDelta` is the raw fragment. `block-end` for **all** blocks is
  deferred to `[DONE]` (`translate.ts:102-117`) so ordering with usage/finish cannot be violated.
- **pi-ai**: pi-ai's `contentIndex` maps 1:1 (`stream.ts:128-131`), but pi-ai hands back **parsed**
  arguments, so `toolcall_end` re-stringifies: `arguments: JSON.stringify(event.toolCall.arguments)`
  (`stream.ts:174-187`, comment at `:182-184`). This is exactly the "twin adapter" divergence the
  Agent Note claims justified the second implementation.
- Truncation safety: `BlockAssembler.blocks()` "max-token truncation drops tool calls that cannot be
  executed safely" (`docs/subsystems/llm-streaming.md:283-286`).

### Thinking / reasoning — the DeepSeek-specific part

Reasoning is a **first-class content block** (`ReasoningBlock`, `types.ts:59-63`) and a first-class
delta (`reasoning-delta`), not a text variant.

DeepSeek wire handling, all in one place:

- `reasoning_content` typed as `string | null` with the documented quirk: *"The FIRST chunk carries
  an empty string (must not open a reasoning block)"* — `types.ts:106-116`
- translation honors it: reasoning is emitted **before** text each chunk (thinking interleaves first),
  and an empty string does not open a block — `translate.ts:130-140`
- **CoT passback**: `reasoning_content` is replayed on assistant history messages **only on
  tool-call turns**, per DeepSeek's thinking-mode guide, and dropped otherwise to save tokens —
  `serialize.ts:96-100`, wire type doc at `types.ts:63-73`
- assistant `content` is `""`, **never `null`** — a documented 400-avoidance for reasoning-only turns
  that would otherwise brick the whole session log (`serialize.ts:86-95`)
- request-side dispatch: `thinking: {type: 'enabled'|'disabled'}` **top-level** (not `extra_body`)
  plus `reasoning_effort: 'high'|'max'` — `types.ts:18-21`, resolution logic `serialize.ts:36-53`
  (harness effort `off` maps to `thinking: disabled` and never appears as a wire effort;
  `purpose: 'session-title'` forces thinking off, `serialize.ts:38`)
- effort catalog surfaced to selectors: `off/High/Max`, default from config —
  `adapter.ts:95-105, 194-210`

pi-ai side: `thinking_start`/`thinking_delta`/`thinking_end` → `reasoning` blocks
(`stream.ts:145-152`); levels come from `getSupportedThinkingLevels(model)`
(`llm-pi-ai/src/adapter.ts:154-169`); a model with no reasoning metadata reports **no** `reasoning`
field at all rather than a fake `off`-only control (rationale `adapter.ts:138-153`);
`off` is translated to *omitting* the pi-ai reasoning option (`adapter.ts:87`).
Reasoning **signatures** (Anthropic `thinkingSignature`, `redacted`, Gemini `thoughtSignature`,
`textSignature`) are preserved through the durable log as adapter-private replay state —
`packages/llm/llm-pi-ai/src/replay.ts:15-31, 63-91`, reconstruction `replay.ts:160-203`,
validation `replay.ts:98-122`.

Replay state is scoped: `LlmRuntime` strips it when the historical provider and target provider are
not owned by the **same adapter instance** (`packages/llm/llm/src/index.ts:822-836`).

### Usage accounting

`TokenUsage` (`types.ts:135-141`): `inputTokens`, `outputTokens`, `cacheReadTokens?`,
`cacheWriteTokens?`, `reasoningTokens?`. Counts are **DISJOINT** by convention — billed input is the
sum of the three input buckets (`types.ts:127-134`), and `reasoningTokens` is informational detail
already inside `outputTokens` (`docs/subsystems/llm-streaming.md:244-248`).

- DeepSeek folds cache hits into `prompt_tokens`, so `mapUsage` **subtracts them back out**:
  `translate.ts:53-62` (`inputTokens = prompt_tokens - cacheRead`).
- pi-ai reports zeros rather than absence, so cache fields are emitted only when `> 0`:
  `stream.ts:22-29`. pi-ai folds reasoning into output, so `reasoningTokens` is not surfaced there.
- Usage may arrive attached to the finish chunk *or* as a trailing usage-only chunk; the latest wins
  and both are deferred to `[DONE]` (`translate.ts:177-179`).
- Downstream: `packages/llm/token-meter` prices the session surface, summing disjoint buckets in
  `usageTokens()` (`token-meter/src/index.ts:44-49`) and folding a signed surface delta against a
  usage or heuristic baseline (`token-meter/src/types.ts:18-36`).

---

## 5. Errors / retry

### Typed taxonomy at the seam, regex-over-text at the provider edge

Base class `HarnessError { code: string }` with cause chaining
(`packages/llm/llm/src/error.ts:13-22`); `LlmError extends HarnessError` adds a validated
serializable `failure: LlmFailure` (`packages/llm/llm/src/index.ts:83-117`), where
`LlmFailure = {message, code, status?, providerRetryAfterMs?, requestId?}` (`types.ts:40-51`).
The rule is stated on the class: *"route on this, never by parsing `message`"* (`error.ts:14`).

Canonical codes exported as constants: `CONTEXT_WINDOW_EXCEEDED`, `QUOTA`, `EMPTY_RESPONSE`,
`INVALID_CREDENTIAL` (`error.ts:24-48`). Codes used in practice: `AUTH`, `RATE_LIMIT`,
`INVALID_REQUEST`, `SERVER`, `TIMEOUT`, `TRANSPORT`, `ABORTED`, `MALFORMED_RESPONSE`,
`STREAM_CLOSED`, `EMPTY_RESPONSE`, `MISSING_CREDENTIAL`, `UNSUPPORTED_CONTENT`,
`UNSUPPORTED_REASONING_EFFORT`, `UNSUPPORTED_OPTION`, `UNKNOWN_MODEL`, `NO_ADAPTER`,
`DUPLICATE_ADAPTER`, `INVALID_CATALOG`, `INVALID_MODEL_*`, `INVALID_PREPARED_CALL`,
`REGISTRATION_DISPOSED`, `INVALID_REPLAY_STATE`, `DISCOVERY_FAILED`, `DISCOVERY_UNSUPPORTED`,
`PI_AI_ERROR`, `HTTP_<status>`.

**But classification into that taxonomy is regex over provider text**, and dsh is candid about it:

- DeepSeek HTTP: status-first, then text — `httpErrorCode()` at
  `packages/llm/llm-deepseek/src/adapter.ts:138-149` (401/403→AUTH, quota-regex→QUOTA, 429→RATE_LIMIT,
  400+context-regex→CONTEXT_WINDOW_EXCEEDED, ≥500→SERVER, else `HTTP_n`).
- pi-ai: **text only**, because pi-ai flattens the caught error to `error.message` and discards the
  `cause` chain — `classifyPiAiError()` at `packages/llm/llm-pi-ai/src/stream.ts:31-62`, with an
  explicit `XXX(pi-ai upstream)` comment saying "we are left pattern-matching terse words here.
  If pi-ai ever forwards the original Error … classify on `code`/`cause` instead of text."
  It regexes for `401|403`, `429|rate limit`, `400|invalid request`, `5\d\d`, `timeout`,
  `stream ended (before|without)`, `network|connection|socket|fetch|ECONN*|terminated|premature close`.
- Shared text classifiers live in the seam so both adapters agree:
  `isContextWindowExceededError()` (5 regexes, `error.ts:51-86`) and `isQuotaExceededError()`
  (5 regexes, `error.ts:94-100`).

### Context-overflow detection — three independent paths

1. HTTP 400 + text match → `CONTEXT_WINDOW_EXCEEDED` (`llm-deepseek/src/adapter.ts:143-145`).
2. pi-ai's own `isContextOverflow(message, contextWindow)` — a **usage-based** check against the
   resolved catalog capacity — OR dsh's text classifier, in `mapStopReason()`
   (`llm-pi-ai/src/stream.ts:73-86`); the doc comment notes zero-output `length` usage that fills the
   window also counts.
3. Proactive: compaction reads `resolveModelInfo().context.contextWindow` and summarizes before
   overflow (`packages/compaction/compaction-basic/src/index.ts:293`); token-meter supplies pressure.

### Retry: policy at the seam, execution in a separate plugin

**Policy** is provider-owned config resolved *before* registration into an immutable union
(`packages/llm/llm/src/retry-policy.ts:59-79`): `mode: 'normal'` (finite `maxRetries` default 2,
`retryableCodes` default `[EMPTY_RESPONSE, RATE_LIMIT, SERVER, TIMEOUT, TRANSPORT]`, backoff) or
`mode: 'always'` (unbounded). Defaults `retry-policy.ts:14-24`; schema `:81-103`;
resolution + key-typo rejection `:111-191`. It is **captured at registration**
(`packages/llm/llm/src/index.ts:387-393`), which is why a policy change forces an atomic
`registration.replace()` (`llm-deepseek/src/index.ts:258-268`, `llm-pi-ai/src/index.ts:253-275`).

**Execution** is a *separate optional plugin*, `packages/llm/llm-retry`, which does not touch the LLM
seam at all — it listens on the agent loop's `agent/request-error` waterfall
(`llm-retry/src/index.ts:210-219`, loop dispatch at `packages/core/agent-loop/src/agent.ts:355-360`).
Backoff: exponential with symmetric jitter, capped (`llm-retry/src/index.ts:58-63`); provider
`Retry-After` honored when ≤ `maxDelayMs`, else the normal policy gives up (`:194-205`);
**each retry is durably appended to the session log before its cancellable wait**
(`llm-retry/src/index.ts:150-153`), and retry counting is recovered by scanning the log
(`:182-192`) so it survives process restarts. Adapters set library retries to zero
(`llm-pi-ai/src/adapter.ts:96-97`: `maxRetries: 0`).

### Adapter-throw normalization

`LlmRuntime.adapterStream()` converts any adapter throw (selection, dispatch, iteration) into a
terminal `finish {kind:'error'|'aborted'}` chunk (`packages/llm/llm/src/index.ts:843-900, 931-939`);
`normalizeLlmFailure()` (`packages/llm/llm/src/adapter-failure.ts:16-104`) defensively reads own
data properties only, never invoking SDK-defined accessors, and refuses to trust foreign `code`
values ("third-party SDK codes are not our taxonomy", `:101-104`).

Idle watchdogs: both adapters arm a per-read idle timer, default **300 s**
(`llm-deepseek/src/adapter.ts:89, 227`; `llm-pi-ai/src/config.ts:35`, use at
`llm-pi-ai/src/adapter.ts:298-299`), mapping expiry to `TIMEOUT` and preserving an earlier caller
abort as `ABORTED`.

---

## 6. Auth / credentials

### The seam is *references*, never values

`packages/credentials/credentials/src/index.ts:1-9`: settings and composition files carry
**environment-variable names**, providers own the values. `CredentialRef` is a branded, validated
POSIX identifier (`index.ts:16-28`). The abstract service has four operations
(`index.ts:60-99`): `resolve(ref)`, `describe(ref)` (returns `{configured, source?, writable}` —
never the value, `index.ts:39-46`), `set`, `unset`. One seam-wide rule: an empty stored value is
absent everywhere (`index.ts:54-59`). Change events: `credentials/updated`
(`credentials/src/types.ts:15-31`).

**Resolution is per-operation, never cached** (`index.ts:65-73`) — this is what makes a rotated key
reach the next request with no restart.

### The one provider: layered env + file

`packages/credentials/credentials-local/src/index.ts:1-36` documents the precedence:

```
inherited process environment      (read-only, WINS)
> $DSH_HOME/.credentials.yaml      (provider-managed, writable)
> <invocation cwd>/.env            (read-only fallback)
> $DSH_HOME/.env                   (read-only fallback)
```

The YAML store is owner-only-enforced (refuses `mode & 0o077`, POSIX only,
`credentials-local/src/index.ts:88-120`), written under a cross-process file lock with
comment-preserving document patching, hot-reloaded via chokidar, replaced wholesale on reload.
Writes are rejected while a read-only layer shadows the ref (`credentials/src/index.ts:83-91`).

### How adapters use it

Both resolve *from the same snapshot that produced the endpoint*, so a request can never pair one
config generation's URL with another's secret:
- DeepSeek: `resolveApiKey(connection)` at `llm-deepseek/src/index.ts:225-246` — tries
  `ctx.get('credentials')` first, falls back to the launch environment when the seam is absent,
  else `MISSING_CREDENTIAL`.
- pi-ai: `llm-pi-ai/src/index.ts:175-198`. A profile naming **no** `apiKeyEnv` defers to pi-ai's own
  ambient discovery; a *named but missing* ref fails loud — with an explicit rationale that handing
  pi-ai `undefined` would let it pick up an unrelated `OPENAI_API_KEY` and bill another tenant
  (`index.ts:180-186`).

Key hygiene is centralized: `normalizeApiKey()` (`packages/llm/llm/src/api-key.ts:36-41`) trims and
enforces printable-ASCII (so `fetch` cannot throw an opaque ByteString error), and
`assertUsableApiKey()` (`packages/llm/llm/src/index.ts:137-152`) produces a diagnostic that names
the *reference* and never echoes any part of the secret.

The key is handed to pi-ai as a **per-request stream option**, never stored in the `Models`
collection (`llm-pi-ai/src/provider.ts:14-18`, `adapter.ts:16-20`); pi-ai treats it as the
highest-priority auth override. For OAuth-only catalog providers, dsh *adds* an api-key auth method
beside the provider's own so an explicit key is honored (`provider.ts:109-135`).

### OAuth / Bedrock / Vertex: **not supported**

- No OAuth flow exists anywhere in dsh (`grep -ri oauth packages --include=*.ts` hits only comments
  in the pi-ai adapter explaining why OAuth providers are withheld).
- Bedrock SigV4, Vertex ADC, Azure api-version: refused as hand-declared protocols
  (`llm-pi-ai/src/provider.ts:36-45`, test `tests/catalog.spec.ts:314-325`).
- A *catalog* route may still use them via pi-ai's own auth (`provider.ts:109-135` keeps the catalog
  provider's `auth` intact), and the Models UI accounts for "authenticates through the provider's own
  path (the Bedrock chain, Vertex ADC…)" by treating a keyless profile as usable
  (`packages/client/ui-settings-models/src/client/store.ts:193-205`).
  But dsh ships no configuration surface for those credentials.

### Attribution headers are mandatory

`attributionHeaders()` → `{'user-agent': 'deepseek-harness/<version> (+repo url)'}`
(`packages/llm/llm/src/attribution.ts:64-68`); version read from the package manifest so it cannot
drift (`:10-16`); "nothing can suppress attribution entirely" (`:34-44`). Deployment headers that
collide are stripped case-insensitively (`llm-pi-ai/src/adapter.ts:171-179`).
DeepSeek also sends `x-deepseek-harness-user-id` (anonymous id), `x-deepseek-harness-session-id`,
and `x-deepseek-harness-compact: 1` for compaction calls (`llm-deepseek/src/adapter.ts:283-295`).

---

## 7. Media

### Image **input**: supported, via a durable attachment service

`ImageBlock` carries an `ImageAttachmentRef`, not bytes (`packages/llm/llm/src/types.ts:65-75`):
`{attachmentId, mediaType, bytes, width, height, name?}`
(`packages/attachment/attachment/src/types.ts:11-24`), with deployment limits
(`maxImageBytes`, `maxImagesPerMessage`, `maxMessageImageBytes`, `maxImagePixels`, `types.ts:27-31`).

- Admission is capability-gated *before* the message becomes durable, using `inputModalities`;
  an explicit omission is **negative capability** (`packages/llm/llm/src/index.ts:655-657`,
  `types.ts:242-243`). The reasoning for defaulting to `['text']` rather than "unknown" is written
  out at `packages/llm/llm-pi-ai/src/config.ts:43-53`: under-claiming refuses the image up front,
  over-claiming admits one that fails mid-turn after the message is durable.
- pi-ai path: base64-inlines the stored bytes at request time
  (`packages/llm/llm-pi-ai/src/context.ts:39-47`), requires the attachment service
  (`llm-pi-ai/src/adapter.ts:302-309`), and walks nested tool-result content
  (`context.ts:29-65`).
- DeepSeek path: **rejects images explicitly** — the chat-completions route is text-only, and
  `assertTextOnly()` refuses rather than letting text-flattening silently erase the image
  (`llm-deepseek/src/serialize.ts:63-68`, `adapter.ts:186-192`).
- One shared recursive walk, `contentHasImage()` (`packages/llm/llm/src/content.ts:9-16`), so no
  consumer can diverge on nesting depth.

### Image **output**: forward-compatible shape, zero implementation

`ImageBlock` is "deliberately role-neutral; assistant-side rendering is forward compatibility — the
current production adapters declare text-only output, so only user content carries images today"
(`packages/llm/llm/src/types.ts:65-70`). The pi-ai replay path throws outright:
`'pi-ai chat history cannot represent structured assistant image output'` → `UNSUPPORTED_CONTENT`
(`packages/llm/llm-pi-ai/src/replay.ts:138-139`).

### Audio: none

`ModelModalityMap` is `{text, image}` only (`types.ts:152-155`). Audio appears only as *discarded*
MCP tool content (`packages/mcp/mcp-client/src/tools.ts:304-305`) and as a declined ACP capability
(`packages/acp/acp/src/index.ts:241`: `promptCapabilities: {image: false, audio: false, ...}`).

### Image generation: none

No generation endpoint, no image-output block emission, nothing in the request vocabulary.

---

## 8. The plugin angle

Yes — the LLM layer is a Cordis service, and **third parties can ship a provider as a dsh plugin
with no changes to dsh**.

### Mechanism

1. **Service declaration by module augmentation.** `packages/llm/llm/src/index.ts:46-67`:
   ```ts
   declare module '@deepseek-ai/cordis' {
     interface Context { llm: LlmRuntime }
     interface Events {
       'llm/stream'(this: LlmRuntime, options: GenerateOptions,
                    next: () => AsyncIterable<StreamChunk>): AsyncIterable<StreamChunk>  // waterfall
     }
   }
   ```
   plus `'llm/adapters-updated'()` (emit) declared in `types.ts:12-25`.
   `LlmRuntime extends Service`, `super(ctx, 'llm')` (`index.ts:284-294`).

2. **A provider plugin is an ordinary Cordis plugin**: export `name`, `inject = ['llm']`, `Config`
   (schemastery), `apply(ctx, config)`. Compare `llm-deepseek/src/index.ts:41-42, 200` with
   `llm-pi-ai/src/index.ts:84-85, 150` — identical shape.

3. **Registration is fiber-scoped and hot-swappable.** `ctx.effect(...)` ties the route set to the
   plugin's lifetime (`packages/llm/llm/src/index.ts:345-354`); HMR/unload releases routes and emits
   `llm/adapters-updated`. `handle.replace(routes)` swaps the route set in **one synchronous
   section**, validated in full first, so no observer sees a gap (`index.ts:398-413`, contract at
   `:239-257`).

4. **Users install it by adding an entry to `cordis.yml`** naming any npm package
   (`examples/headless-agent/cordis.yml:20-21` — `name: '@deepseek-ai/dsh-llm-deepseek'`).
   Nothing in the seam special-cases first-party packages.

5. **Three optional registries a provider plugin can also join**, all seam-level:
   - `registerConfigurableProviders(entries)` — publish a settings address for a dormant route
     (`index.ts:431-484`)
   - `registerModelDiscovery(settingsNs, discover)` — offer "fetch available models" for drafts
     (`index.ts:504-521`)
   - `providerRetryPolicy(provider)` — hand the seam an immutable policy that `dsh-llm-retry`
     will execute (`index.ts:195-197`)

6. **Service injection is optional and runtime-probed.** Adapters call `ctx.get('credentials')` /
   `ctx.get('attachments')` and degrade gracefully when the seam is unmounted
   (`llm-deepseek/src/index.ts:229-240`, `llm-pi-ai/src/index.ts:203`).
   Only `inject = ['llm']` is hard.

7. **Middleware, not just providers.** `llm/stream` is a **waterfall**: any plugin may wrap every
   model call, call `next()` to reach the adapter, or yield its own chunks to short-circuit
   (`index.ts:52-65`, terminal continuation at `index.ts:917-927`). Adapter *lookup* happens at the
   terminal continuation, so a listener can route or serve an unregistered route
   (`docs/subsystems/llm-streaming.md:629`). `packages/test-support/llm-replay` exploits exactly this
   to replay recorded sessions keylessly (`llm-replay/src/index.ts:750`).

8. **Two independent extension axes on top**: `ContentBlockMap` and `FinishReasonMap` /
   `MessageSourceMap` are merge-extensible interfaces, so a plugin can add a block type
   (`types.ts:95-110, 112-125`) — with the stated cost that "new core blocks must land with adapter,
   UI, and compaction support" (`types.ts:95-98`).

The seam is defensive about untrusted plugin code: adapter-returned metadata is re-validated and
detached on every path (`INVALID_CATALOG` / `INVALID_MODEL_INFO` / `INVALID_MODEL_CONTEXT` /
`INVALID_MODEL_MAX_TOKENS` / `INVALID_MODEL_REASONING`, `index.ts:581-718`), listener failures on
registry events are contained rather than allowed to veto a commit (`index.ts:296-328`), and
`GenerateOptions` reaching the waterfall from the agent loop is **deep-frozen**
(`call-config.ts:88-117`, freeze at `index.ts:782-785`).

---

## 9. Miscellaneous facts worth carrying into the comparison

- **`GenerateOptions`** (`types.ts:320-356`) is small: `provider`, `model`, `reasoningEffort?`,
  `messages`, `system?`, `tools?`, `temperature?`, `maxTokens?`, `stop?`, `signal?`, `sessionId?`,
  `purpose?: 'compaction' | 'session-title'`. No top_p, no seed, no response_format, no
  structured-output/JSON-mode, no tool_choice, no parallel_tool_calls, no cache-control at request
  level. `stop` is unsupported by the pi-ai adapter at all (`llm-pi-ai/src/adapter.ts:277-279`).
- **`LlmCallConfig`** (`call-config.ts:23-30`) is the cache-affecting subset that gets logged as a
  durable request header; changes are detected field-wise (`callConfigEquals`, `:49-59`) and appended
  to the session log (`packages/core/agent-loop/src/agent.ts:454-470`).
- **`prepareCall()`** (`index.ts:779-814`) is a one-shot handle: it freezes config + registration +
  context, reports which fields the *adapter* defaulted (`adapterDefaults`), and refuses reuse or a
  changed config at dispatch (`INVALID_PREPARED_CALL`). This is the seam's answer to "the user
  switched models mid-reply".
- **System prompt** is a single `system?: string` slot; pi-ai's single `systemPrompt` matches, and
  in-history system messages are folded into user messages (`llm-pi-ai/src/context.ts:148-156`).
- **Tool results** are modeled as blocks inside *user* messages harness-side, expanded into
  `role: 'tool'` (DeepSeek, `serialize.ts:126-138`) or `role: 'toolResult'` (pi-ai,
  `context.ts:107-119`) wire messages; empty output becomes the literal `'(no output)'` in both
  (`serialize.ts:136`, `context.ts:114-116`).
- **Config validation is fail-soft at runtime, fail-loud at load**: a bad settings snapshot keeps the
  last good configuration and logs once (`llm-deepseek/src/index.ts:212-221`,
  `llm-pi-ai/src/index.ts:286-309`), while a bad composition entry throws at load.
  The pi-ai plugin also registers `assertServiceable` as the settings **validator** so an
  unserviceable profile is refused *where it is written* (`llm-pi-ai/src/config.ts:271-273`,
  wired at `index.ts:282`).
- **Generated docs**: `docs/config-catalog.md:839-1082` renders both plugins' config schemas from
  source; `docs/subsystems/llm-streaming.md` (888 lines) is the seam reference and includes a
  generated Cordis API section.

---

## 10. vs pi-ai vs our BAML plan

### Structural comparison

| Axis | pi-ai (badlogic/pi-mono) | dsh | our BAML native-provider plan |
|---|---|---|---|
| Wire implementations | 10 adapters | **1** in-repo (DeepSeek chat-completions) + 3 reachable via pi-ai's table + N more via pi-ai catalog providers | typed request/response classes per **endpoint**, thin per-provider classes over shared per-API-family cores |
| Provider unit | config record (×40) | route string → `LlmAdapter` **class instance**; inside the pi-ai plugin, a config record in a `providers` dict | thin **class** per provider over a shared core |
| Compat flags | ~30 typed flags | **2** exposed (`thinkingFormat`, `supportsReasoningEffort`), rest left to pi-ai's baseURL auto-detection (`catalog.ts:186-199`) | (n/a — divergence expressed as class/method overrides) |
| Catalog | models.dev-generated | **no generator**; pi-ai's builtin catalog at runtime + hand-written 2-model DeepSeek default + user config + live `GET /models` | TBD |
| Cost/pricing | in catalog | **absent by design** (`catalog.ts:27-32`) | TBD |
| Auth | provider-config | reference-only seam; api-key only; **no OAuth/SigV4/ADC** | "auth-only Rust seams" — same instinct |
| Payload typing | zod-ish/TS types | hand-written TS interfaces per wire (`llm-deepseek/src/types.ts`), no runtime response validation; schemastery only for **config** | typed classes per endpoint, checked by the BAML type system |

### What dsh is actually evidence *for*

1. **A two-registry design (adapters + configurable-provider directory) is what makes a settings UI
   possible.** pi-ai has provider records; dsh had to invent `LlmConfigurableProvider` with a
   `settingsNs` + `settingsPath` so a UI can address a dormant provider's config *before* it exists
   (`types.ts:160-187`). If our plan wants a "configure a provider" surface, the provider record
   needs a settings address, not just a definition.
2. **"Catalog is advisory, never a whitelist" is repeated as a rule, three times.** Our typed
   per-endpoint classes must not turn model metadata into request validation, or every new model
   release breaks users.
3. **Wrapping someone else's provider layer costs you the error taxonomy.** dsh's most-apologetic
   code is `classifyPiAiError()` (`stream.ts:31-62`), regexing English prose because pi-ai flattened
   the `Error` and dropped `cause`. That is a direct argument for our plan's typed error surface —
   the taxonomy has to be produced *at the wire boundary*, by the code that saw the HTTP status and
   the body, or it degenerates into regex.
4. **Reasoning needs three separate concepts, not one flag.** dsh models: a `reasoning` *content
   block*, a `reasoning-delta` *event*, and an *effort/level capability list* per model with a wire
   spelling map (`PiAiReasoningEfforts`, `catalog.ts:176-183` — keys are selectable levels, values
   are wire strings). Plus **signature passback** as opaque adapter-private replay state
   (`replay.ts:15-31`). A design with only `reasoning: bool` cannot express DeepSeek's
   `thinking + reasoning_effort + reasoning_content-on-tool-turns-only` rule
   (`serialize.ts:36-53, 96-100`) *or* Anthropic's thinking signatures.
5. **Streaming needs a "raw JSON string end-to-end" rule for tool arguments.** dsh states it as a
   contract (`docs/subsystems/llm-streaming.md:206-207`) precisely because pi-ai parses and dsh must
   re-stringify (`stream.ts:182-184`), losing key order and number fidelity. Our typed request/response
   classes should keep tool arguments as a string until the tool boundary.
6. **Usage buckets must be declared disjoint or not.** dsh chose disjoint and pays for it by
   subtracting DeepSeek's cache hits back out (`translate.ts:53-62`) and by discarding pi-ai's zeros
   (`stream.ts:22-29`). Pick a convention and write it on the type.
7. **The one place dsh beats both**: retry is *policy at the provider* + *execution in a separate
   plugin that writes each attempt to a durable log before waiting*
   (`llm-retry/src/index.ts:150-153, 182-192`). Retry count survives a process restart. That is a
   nicer factoring than burying backoff inside a client class, and it maps cleanly onto a BAML
   design where the client declares policy and the runtime executes it.
8. **Two implementations against one vocabulary is a cheap correctness gate.** The twin-adapter note
   (`2026-06-13-twin-llm-adapters.md:13-18`) claims the second adapter is what surfaced the
   two-error-paths problem and the parsed-vs-raw tool arguments problem. If our plan builds
   per-API-family cores, having at least two *different-shaped* families implemented before the
   vocabulary is frozen is the same gate.

### Where dsh is a weaker model than our plan

- **No pricing, no structured output, no tool_choice, no cache-control, no audio, no image output,
  no image generation, no embeddings.** dsh's request vocabulary (`types.ts:320-356`) is a coding
  agent's minimum, not a general LLM client.
- **No runtime validation of provider responses at all** — `JSON.parse(x) as WireChunk`
  (`translate.ts:120-125`). Our typed classes can do strictly better essentially for free.
- **Auth is api-key-or-nothing.** Bedrock/Vertex/Azure/Codex are documented as out of scope
  (`provider.ts:36-45`). Our "auth-only Rust seams" idea is the *right* shape but has to actually
  cover signed requests, which dsh punts on entirely.
