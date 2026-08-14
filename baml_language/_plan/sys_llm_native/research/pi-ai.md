# `@mariozechner/pi-ai` — LLM Provider Architecture Map

Research notes for the `sys_llm_native` design work.

- **Package**: `@earendil-works/pi-ai` v0.84.1 (the npm name `@mariozechner/pi-ai` is the older
  publishing identity; the repo is `github.com/earendil-works/pi`, directory `packages/ai`).
- **Source read**: `/private/tmp/.../scratchpad/pi-mono/packages/ai` (read-only checkout).
- **All paths below are relative to `packages/ai/`.** Line numbers are from this checkout.
- Author: Mario Zechner. License MIT. Node ≥ 22.19.

---

## 0. TL;DR — the shape of the thing

The user's framing — *"they just have `openai.responses()` adapters and things, and the client
is mostly just config"* — is **substantially correct, with three important qualifications.**

**Correct:** there are exactly **10 wire adapters** (`KnownApi`, `src/types.ts:17-27`), every one
of them exports the identical two-function module contract, and **40 providers** are assembled
from a ~15-line `createProvider({ id, name, baseUrl, auth, models, api })` record. 24 of the 40
providers are literally nothing but `{id, name, baseUrl, envApiKeyAuth(...), MODELS, api}`.

**Qualification 1 — the quirks did not disappear, they moved into a typed `compat` struct.**
Provider-specific misbehavior is modeled as ~30 boolean/enum flags on `Model.compat`
(`src/types.ts:545-687`), auto-defaulted by **substring-matching the `baseUrl`**
(`src/api/openai-completions.ts:1444-1538`). This is the single most important design decision in
the package and the thing a "provider is just config" summary hides.

**Qualification 2 — the code mass is overwhelmingly in the adapters, not the config.**

| Layer | Lines | Files |
|---|---|---|
| `src/api/**` (wire adapters) | **11,381** | 32 |
| `src/providers/**` minus `faux.ts`/`all.ts` | **~1,200** | 45 |
| `src/models.ts` (collection/dispatch) | 944 | 1 |
| `src/types.ts` | 830 | 1 |

Four adapters carry two-thirds of the total: `openai-codex-responses.ts` (1662),
`openai-completions.ts` (1577), `anthropic-messages.ts` (1352), `bedrock-converse-stream.ts` (1188).

**Qualification 3 — three providers escape the config pattern via hand-written `ApiKeyAuth`**
(bedrock, vertex, cloudflare), and one escapes via a `ProviderStreams` decorator (cloudflare).
Details in §4.

---

## 1. The adapter/config split

### 1.1 The ten wire adapters

`KnownApi` — `src/types.ts:17-27`:

```ts
export type KnownApi =
  | "openai-completions"      | "mistral-conversations"
  | "openai-responses"        | "azure-openai-responses"
  | "openai-codex-responses"  | "anthropic-messages"
  | "bedrock-converse-stream" | "google-generative-ai"
  | "google-vertex"           | "pi-messages";

export type Api = KnownApi | (string & {});   // src/types.ts:29 — open for custom adapters
```

Plus one image API family: `KnownImagesApi = "openrouter-images"` (`src/types.ts:31-33`).

| Adapter | file | LOC | transport | SDK |
|---|---|---|---|---|
| `openai-completions` | `src/api/openai-completions.ts` | 1577 | SDK-decoded SSE | `openai` |
| `openai-responses` | `src/api/openai-responses.ts` (+`-shared.ts` 792) | 372 | SDK-decoded SSE | `openai` |
| `azure-openai-responses` | `src/api/azure-openai-responses.ts` | 330 | SDK-decoded SSE | `openai` (`AzureOpenAI`) |
| `openai-codex-responses` | `src/api/openai-codex-responses.ts` | 1662 | **SSE + WebSocket** | hand-rolled `fetch` |
| `anthropic-messages` | `src/api/anthropic-messages.ts` | 1352 | **hand-rolled SSE** over SDK request | `@anthropic-ai/sdk` |
| `bedrock-converse-stream` | `src/api/bedrock-converse-stream.ts` | 1188 | AWS event stream | `@aws-sdk/client-bedrock-runtime` |
| `google-generative-ai` | `src/api/google-generative-ai.ts` (+`-shared.ts` 419) | 521 | SDK async iterable | `@google/genai` |
| `google-vertex` | `src/api/google-vertex.ts` | 596 | SDK async iterable | `@google/genai` (`vertexai:true`) |
| `mistral-conversations` | `src/api/mistral-conversations.ts` | 931 | **hand-rolled `fetch` + SSE** | none |
| `pi-messages` | `src/api/pi-messages.ts` | 433 | **hand-rolled `fetch` + SSE** | none |

Note the naming trap: **`mistral-conversations` does not call Mistral's `/v1/conversations`
Agents API.** It POSTs to `new URL("v1/chat/completions", baseUrl)`
(`src/api/mistral-conversations.ts:289-291`); the module doc at `:118-120` says "native Mistral
Chat Completions endpoint". The api id is just a pi-internal label.

`pi-messages` is not a vendor API at all — it is **pi's own wire protocol** (`src/api/pi-messages.ts:1-10`):
POST `{model, context, options}` to `<baseUrl>/messages`, receive SSE carrying pi's *own*
serialized `AssistantMessageEvent`s. It is a remote-provider/gateway proxy protocol; the server
does the provider-specific translation. Only `radius` uses it (`src/providers/radius.ts:25`).

### 1.2 The adapter interface contract

Every module under `src/api/` exports exactly the same two symbols. The contract is declared as
`ProviderStreams` (`src/types.ts:268-277`):

```ts
export interface ProviderStreams {
  stream(model: Model<Api>, context: Context, options?: StreamOptions): AssistantMessageEventStream;
  streamSimple(model: Model<Api>, context: Context, options?: SimpleStreamOptions): AssistantMessageEventStream;
  fetchDeferred?(model, handle: DeferredHandle, options?): AssistantMessageEventStream;
  cancelDeferred?(model, handle: DeferredHandle, options?): Promise<void>;
}
```

and typed per-adapter as `StreamFunction<TApi, TOptions>` (`src/types.ts:320-324`):

```ts
export type StreamFunction<TApi extends Api = Api, TOptions extends StreamOptions = StreamOptions> = (
  model: Model<TApi>, context: Context, options?: TOptions,
) => AssistantMessageEventStream;
```

Concrete exports, all verified:

| adapter | `stream` | `streamSimple` |
|---|---|---|
| openai-completions | `:201-615` | `:617-635` |
| openai-responses | `:102-195` | `:197-212` |
| azure-openai-responses | `:68-159` | `:161-179` |
| openai-codex-responses | `:244-477` | `:506-528` |
| anthropic-messages | `:487-774` | `:801-841` |
| bedrock-converse-stream | `:107-459` | `:461-507` |
| google-generative-ai | `:52-293` | `:296-300` |
| google-vertex | `:70-310` | `:313-317` |
| mistral-conversations | `:121-178` | `:180-201` |
| pi-messages | `:345-419` | `:421-425` |

**Takes:** `(model, context, options)`. `Model<TApi>` (`src/types.ts:794-823`) carries the endpoint
+ capabilities; `Context` (`src/types.ts:509-513`) is `{systemPrompt?, messages, tools?}`;
`options` is the API-specific option type.

**Returns:** an `AssistantMessageEventStream` **synchronously**. The contract comment at
`src/types.ts:312-319` is explicit:

> Once invoked, request/model/runtime failures should be encoded in the returned stream, not
> thrown. Error termination must produce an `AssistantMessage` with stopReason `"error"` or
> `"aborted"` and errorMessage, emitted via the stream protocol.

**`stream` vs `streamSimple`:** `stream` takes provider-native knobs (Anthropic
`thinkingBudgetTokens`, Google `thinking.budgetTokens`, OpenAI `reasoningEffort`);
`streamSimple` takes the unified `SimpleStreamOptions` (`src/types.ts:304-310`) with a portable
`reasoning?: ThinkingLevel` and translates. All ten `streamSimple`s funnel through
`buildBaseOptions()` (`src/api/simple-options.ts:21-52`), which also clamps `maxTokens` against the
context window with a 4096-token safety margin (`:12-19`).

**Deferred responses are declared but unimplemented.** `fetchDeferred`/`cancelDeferred` appear in
the type (`src/types.ts:271-276`), the dispatcher (`src/models.ts:706-731`, `:835-857`), and the
lazy wrapper (`src/api/lazy.ts:81-95`) — but **no real adapter implements them**. The only
implementation is the test double `src/providers/faux.ts:567` / `:633`.

### 1.3 Provider = config over an adapter

`Provider` (`src/models.ts:97-149`) and its factory `createProvider(input: CreateProviderOptions)`
(`src/models.ts:739-862`):

```ts
export interface CreateProviderOptions<TApi extends Api = Api> {
  id: string;
  name?: string;                                   // default: id
  baseUrl?: string;
  headers?: ProviderHeaders;
  auth: ProviderAuth;                              // required — even keyless local servers
  models: readonly Model<TApi>[];                  // static baseline catalog
  fetchModels?: (ctx: RefreshModelsContext) => Promise<readonly Model<TApi>[]>;
  filterModels?: (models, credential) => readonly Model<TApi>[];
  api: ProviderStreams | Partial<Record<TApi, ProviderStreams>>;   // single, or map keyed by model.api
}
```

Dispatch is 12 lines (`src/models.ts:775-792`): if `api` has a `.stream` function it is the single
implementation, otherwise it is a map indexed by `model.api`; an unmatched api yields a stream
error rather than a throw.

A representative provider in full — `src/providers/groq.ts:1-15`:

```ts
import { openAICompletionsApi } from "../api/openai-completions.lazy.ts";
import { envApiKeyAuth } from "../auth/helpers.ts";
import { createProvider, type Provider } from "../models.ts";
import { GROQ_MODELS } from "./groq.models.ts";

export function groqProvider(): Provider<"openai-completions"> {
  return createProvider({
    id: "groq",
    name: "Groq",
    baseUrl: "https://api.groq.com/openai/v1",
    auth: { apiKey: envApiKeyAuth("Groq API key", ["GROQ_API_KEY"]) },
    models: Object.values(GROQ_MODELS),
    api: openAICompletionsApi(),
  });
}
```

**`together.ts`, `cerebras.ts`, `nvidia.ts`, `deepseek.ts`, `baseten.ts`, `huggingface.ts`,
`moonshotai.ts`, `zai.ts`, `xiaomi.ts`, `openrouter.ts`, `qwen-token-plan*.ts` are byte-for-byte
the same template** with a different id/URL/env-var. That is the "provider is just config" claim,
and it is literally true for 24 of 40.

### 1.4 The full provider → adapter mapping

Extracted mechanically from `src/providers/*.ts`:

| provider | adapter(s) | baseUrl |
|---|---|---|
| `openai` | openai-responses | `https://api.openai.com/v1` |
| `anthropic` | anthropic-messages | `https://api.anthropic.com` |
| `google` | google-generative-ai | `https://generativelanguage.googleapis.com/v1beta` |
| `google-vertex` | google-vertex | *(none — SDK/region-derived)* |
| `amazon-bedrock` | bedrock-converse-stream | *(none — per-model, region-derived)* |
| `mistral` | mistral-conversations | `https://api.mistral.ai` |
| `azure-openai-responses` | azure-openai-responses | *(none — env/resource-derived)* |
| `openai-codex` | openai-codex-responses | `https://chatgpt.com/backend-api` |
| `radius` | **pi-messages** | *(gateway, dynamic)* |
| `openrouter` | openai-completions | `https://openrouter.ai/api/v1` |
| `groq` | openai-completions | `https://api.groq.com/openai/v1` |
| `cerebras` | openai-completions | `https://api.cerebras.ai/v1` |
| `deepseek` | openai-completions | `https://api.deepseek.com` |
| `nvidia` | openai-completions | `https://integrate.api.nvidia.com/v1` |
| `together` | openai-completions | `https://api.together.ai/v1` |
| `baseten` | openai-completions | `https://inference.baseten.co/v1` |
| `huggingface` | openai-completions | `https://router.huggingface.co/v1` |
| `moonshotai` / `-cn` | openai-completions | `https://api.moonshot.ai/v1` / `.cn/v1` |
| `zai` / `zai-coding-cn` | openai-completions | `https://api.z.ai/api/coding/paas/v4` / `open.bigmodel.cn/...` |
| `xiaomi` + 3 token-plan | openai-completions | `https://api.xiaomimimo.com/v1`, `token-plan-{cn,ams,sgp}...` |
| `qwen-token-plan` ×3 | openai-completions | `https://token-plan.{ap-southeast-1,cn-beijing}.maas.aliyuncs.com/...` |
| `ant-ling` | openai-completions | `https://api.ant-ling.com/v1` |
| `cloudflare-workers-ai` | openai-completions | *(templated, §4.4)* |
| `minimax` / `-cn` | **anthropic-messages** | `https://api.minimax.io/anthropic` / `api.minimaxi.com/anthropic` |
| `kimi-coding` | **anthropic-messages** | `https://api.kimi.com/coding` |
| `vercel-ai-gateway` | **anthropic-messages** | `https://ai-gateway.vercel.sh` |
| `xai` | openai-completions **+** openai-responses | `https://api.x.ai/v1` |
| `fireworks` | anthropic-messages **+** openai-completions | `https://api.fireworks.ai/inference` |
| `github-copilot` | anthropic + completions + responses | `https://api.individual.githubcopilot.com` |
| `opencode` | anthropic + google + completions + responses | *(per-model)* |
| `opencode-go` | anthropic + completions + responses | *(per-model)* |
| `cloudflare-ai-gateway` | anthropic + completions + responses (decorated) | *(templated, §4.4)* |

Observations worth carrying into design:

- **`openai-completions` is the workhorse**: 22 of 40 providers.
- **`anthropic-messages` is the second lingua franca** — MiniMax, Kimi, Vercel AI Gateway,
  Fireworks, Copilot, OpenCode all expose Anthropic-compatible endpoints. "Speaks Anthropic" is
  now a real interop surface, not just Anthropic.
- **Multi-adapter providers are common** (7 of 40) and are exactly why `api` accepts a map keyed
  by `model.api`. The model record decides the wire format, not the provider.
- **Ollama / vLLM / LM Studio are not built-in providers at all** — they are the documented
  `createProvider()` use case (README §Custom Providers), with `compat.supportsDeveloperRole:false`
  and `compat.supportsReasoningEffort:false`.

### 1.5 Lazy loading of adapters

Every provider references its adapter through a 4-line `.lazy.ts` wrapper, e.g.
`src/api/openai-completions.lazy.ts:4`:

```ts
export const openAICompletionsApi = (): ProviderStreams => lazyApi(() => import("./openai-completions.ts"));
```

`lazyApi()` (`src/api/lazy.ts:73-98`) wraps a dynamic-import thunk as a `ProviderStreams`, so
building all 40 providers does not pull in the `openai`, `@anthropic-ai/sdk`, `@google/genai`, and
AWS SDK dependency graphs. `lazyStream()` (`src/api/lazy.ts:46-61`) returns a stream synchronously
while async setup (auth resolution, module load) runs behind it, converting setup failures into a
terminal `error` event with a synthetic `AssistantMessage` (`:4-23`) — this is what makes the
"never throw after invocation" contract hold even for auth failures.

`bedrock-converse-stream.lazy.ts:10-13` is the special case: the specifier is stored in a
**variable** so bundlers cannot statically follow it into the Node-only AWS SDK, with a `.ts`→`.js`
rewrite and `setBedrockProviderModule()` (`:22-24`) as the Bun-compile escape hatch.

---

## 2. The model catalog

### 2.1 Where it comes from

`src/models.generated.ts` is a pure aggregator (auto-generated header at `:1-2`) that imports 40
per-provider constants and exposes `MODELS` (`:44-124`). Each shard is itself generated and 8 lines
long — `src/providers/openai.models.ts:1-8`:

```ts
import values from "./data/openai.json" with { type: "json" };
import { flattenModelCatalog, type ModelCatalog } from "../model-catalog.ts";
export const OPENAI_MODELS: ModelCatalog<typeof values, "openai"> = flattenModelCatalog("openai", values);
```

`src/providers/data/*.json` is **gitignored** (`.gitignore`: `packages/ai/src/providers/data/`) and
hydrated by `npm run generate-models`. The JSON is grouped by API id so TypeScript can derive the
exact `model-id → api` mapping from the JSON keys (`scripts/generate-models.ts:2796-2812`,
`src/model-catalog.ts:10-20`); `flattenModelCatalog` (`src/model-catalog.ts:22-27`) flattens the
groups back into one record at runtime.

**Primary source: models.dev.** `scripts/generate-models.ts:1314-1316`:

```ts
console.log("Fetching models from models.dev API...");
const response = await fetch("https://models.dev/api.json");
```

The models.dev record shape is typed at `scripts/generate-models.ts:78-113` (`ModelsDevModel`:
`tool_call`, `structured_output`, `reasoning`, `reasoning_options`, `limit.{context,output}`,
`cost.{input,output,cache_read,cache_write,tiers}`, `modalities.{input,output}`).

Secondary live sources, fetched directly: OpenRouter `/api/v1/models` (`:1021`), NVIDIA NIM
`/models` (`:999`), Vercel AI Gateway `/models` (`:1083`). A hard filter drops non-tool-calling
models: `if (m.tool_call !== true) continue;` (e.g. `:1367`, `:1469`) — matching the README's
"This library only includes models that support tool calling".

A `.manifest.json` with a `structureHash` (`scripts/model-data.ts:5-17`) guards drift; `npm run
check:model-data` runs in the build (`package.json:52`).

### 2.2 The model entry

`Model<TApi>` — `src/types.ts:794-823`:

```ts
export interface Model<TApi extends Api> {
  id: string;  name: string;  api: TApi;  provider: ProviderId;
  baseUrl: string;
  reasoning: boolean;
  thinkingLevelMap?: ThinkingLevelMap;   // pi level -> provider value; null = unsupported
  input: ("text" | "image")[];
  cost: ModelCost;                       // $/million tokens, + optional tiers
  contextWindow: number;
  maxTokens: number;
  samplingParams?: Record<string, unknown>;
  headers?: Record<string, string>;
  compat?: /* conditional on TApi: OpenAICompletionsCompat | OpenAIResponsesCompat
                                  | AnthropicMessagesCompat | BedrockCompat */;
}
```

`compat` is **conditionally typed on the api** (`src/types.ts:814-822`) — a nice trick: an
`anthropic-messages` model cannot accidentally carry `maxTokensField`.

Cost model (`src/types.ts:776-791`): `ModelCostRates {input, output, cacheRead, cacheWrite}` in
$/M-tokens, plus `ModelCost.tiers?: ModelCostTier[]` where each tier has `inputTokensAbove` and
the highest matching threshold applies to the whole request.

A generated entry (from `scripts/generate-models.ts:1369-1385`, the Anthropic branch):

```ts
{
  id: modelId,
  name: m.name || modelId,
  api: "anthropic-messages",
  provider: "anthropic",
  baseUrl: "https://api.anthropic.com",
  reasoning: m.reasoning === true,
  input: m.modalities?.input?.includes("image") ? ["text", "image"] : ["text"],
  cost: { input: m.cost?.input || 0, output: m.cost?.output || 0,
          cacheRead: m.cost?.cache_read || 0, cacheWrite: m.cost?.cache_write || 0 },
  contextWindow: m.limit?.context || 4096,
  maxTokens: m.limit?.output || 4096,
}
```

So the flags the question asks about map as: `reasoning` ← models.dev `reasoning`; image input ←
`modalities.input`; cost ← `cost.*` including `tiers`; context window ← `limit.context`;
tool calling ← **not a flag, a filter** (non-tool-calling models are excluded entirely).

`thinkingLevelMap` (`src/types.ts:82-84`) is derived from models.dev `reasoning_options` via
`getEffortThinkingLevelMap` (`scripts/generate-models.ts:503-504`), with a large table of
hand-maintained corrections above it.

### 2.3 How model flags change adapter runtime behavior

This is the load-bearing part. Verified consumption sites:

**`reasoning: boolean`** — gates every thinking branch. In `openai-completions.ts` it gates all
eleven `thinkingFormat` branches (`:749, 762, 770, 775, 780, 797, 807, 817, 822, 831, 838, 841`),
the vLLM thinking-budget block (`:852`), the `developer`-vs-`system` role choice (`:1084`), and the
DeepSeek empty-`reasoning_content` injection (`:1225`). In `openai-responses.ts:319` it gates the
whole `reasoning` request param. It is also the gate in `getSupportedThinkingLevels`
(`src/models.ts:903`: `if (!model.reasoning) return ["off"]`).

**`input: ("text"|"image")[]`** — tool-result images are only forwarded when
`model.input.includes("image")` (`openai-completions.ts:1279`, `openai-responses-shared.ts:88`).
More importantly, `src/api/transform-messages.ts:35-56` **silently downgrades** images to text
placeholders (`"(image omitted: model does not support images)"`) for non-vision models, rather
than erroring.

**`contextWindow` / `maxTokens`** — used only by `streamSimple`, via
`clampMaxTokensToContext(model, context, maxTokens)` (`src/api/simple-options.ts:15-19`):
`min(maxTokens, max(1, contextWindow − estimateContextTokens(context) − 4096))`. `contextWindow` is
also read by the *silent*-overflow detector (`src/utils/overflow.ts:145-160`).

**`cost`** — read exclusively by `calculateCost(model, usage)` (`src/models.ts:878-898`), called
from each adapter's usage parser: `openai-completions.ts:1409`, `openai-responses-shared.ts:576`,
`anthropic-messages.ts:586` and `:743`, `google-generative-ai.ts:242`, `bedrock` `handleMetadata`,
`mistral-conversations.ts:603`, `openrouter-images.ts:186-194`.

**`thinkingLevelMap`** — per-level provider-value lookup in every thinking branch
(`openai-completions.ts:756-757, 765, 791-792, 800, 805, 812, 815, 818, 829, 834, 836, 840, 842-844`;
`openai-responses.ts:322, 329-331`) plus `clampThinkingLevel` (`src/models.ts:913-932`), which walks
up then down the level ladder to the nearest supported level.

**`samplingParams`** — merged under per-request params (`simple-options.ts:27-30`) then
`Object.assign`ed onto the request body **last** so custom keys win
(`openai-completions.ts:886-888`, `openai-responses.ts:338-340`). Explicitly OpenAI-family only
(`src/types.ts:183-189`).

**`headers`** — seeds the default header map (`openai-completions.ts:646`,
`anthropic-messages.ts` merge order `:264-272`), and is merged into `Models.getAuth(model)`
(`src/models.ts:555-562`).

**`provider` is behavioral, not cosmetic** — a notable smell: `github-copilot` triggers dynamic
headers (`openai-completions.ts:647-654`), `opencode-go` triggers a reasoning-signature field
remap (`:508`, `:1171`), `openai` triggers 40-char tool-id truncation (`:1077`), `xai` forces
encrypted-reasoning includes (`openai-responses.ts:334`).

---

## 3. Per-provider quirk handling — the `compat` mechanism

### 3.1 The flags

Three compat structs, one per API family:

- `OpenAICompletionsCompat` — **`src/types.ts:545-605`**, ~25 fields.
- `OpenAIResponsesCompat` — `src/types.ts:608-625`, 8 fields.
- `AnthropicMessagesCompat` — `src/types.ts:628-681`, 9 fields.
- `BedrockCompat` — `src/types.ts:684-687`, 1 field.

The completions struct is where the ugliness lives, and it is worth reading as a catalogue of
real-world LLM-endpoint divergence:

| flag | what it papers over |
|---|---|
| `supportsStore` | `store` field rejected by non-OpenAI endpoints |
| `supportsDeveloperRole` | `developer` vs `system` role |
| `supportsReasoningEffort` | `reasoning_effort` unsupported |
| `supportsUsageInStreaming` | `stream_options.include_usage` unsupported |
| `supportsFinishReason` | streams that never send `finish_reason` |
| `maxTokensField` | `max_tokens` vs `max_completion_tokens` |
| `requiresToolResultName` | tool results need a `name` field |
| `requiresAssistantAfterToolResult` | user-after-toolresult needs an assistant turn wedged in |
| `requiresThinkingAsText` | thinking blocks must be flattened to text |
| `requiresReasoningContentOnAssistantMessages` | DeepSeek needs empty `reasoning_content` on replay |
| **`thinkingFormat`** | **11 different reasoning dialects** (see below) |
| `chatTemplateKwargs` / `chatTemplateArgs` | vLLM/Baseten chat-template plumbing, with `{"$var":"thinking.enabled"}` placeholders |
| `cacheControlFormat: "anthropic"` | Anthropic-style `cache_control` markers over an OpenAI-shaped body |
| `sessionAffinityFormat` | 3 different session-pinning header conventions |
| `openRouterRouting` / `vercelGatewayRouting` | gateway routing preference objects |
| `zaiToolStream`, `supportsThinkingTokenBudget`, `supportsOpenAIGrammarTools`, `supportsStrictMode`, `supportsLongCacheRetention`, `deferredToolsMode` | assorted |

`thinkingFormat` (`src/types.ts:567-578`) is the single best illustration of the problem domain —
eleven encodings of "please think harder":

```
"openai"            reasoning_effort: <level>
"openrouter"        reasoning: { effort }
"deepseek"          thinking: { type } + reasoning_effort
"together"          reasoning: { enabled } + reasoning_effort
"baseten"           chat_template_args + reasoning_effort
"zai"               thinking: { type }
"qwen"              enable_thinking: boolean
"qwen-chat-template" chat_template_kwargs.enable_thinking + preserve_thinking
"chat-template"     configurable chat_template_kwargs
"string-thinking"   thinking: "<string>"
"ant-ling"          reasoning: { effort } only when mapped effort is non-null
```

### 3.2 Auto-detection from `baseUrl`

`detectCompat(model)` — **`src/api/openai-completions.ts:1444-1538`** — and the merge
`getCompat(model)` at **`:1544-1577`** (`model.compat?.X ?? detected.X` field by field).

Predicates (`:1448-1493`), each matching on `model.provider` **or** a `baseUrl` substring:

| line | predicate | matches |
|---|---|---|
| `:1448-1452` | `isZai` | provider `zai`/`zai-coding-cn`, `api.z.ai`, `open.bigmodel.cn` |
| `:1453-1454` | `isTogether` | `together`, `api.together.ai`, `api.together.xyz` |
| `:1455` | `isMoonshot` | `moonshotai`/`-cn`, `api.moonshot.` |
| `:1456` | `isOpenRouter` | `openrouter`, `openrouter.ai` |
| `:1457` | `isCloudflareWorkersAI` | `cloudflare-workers-ai`, `api.cloudflare.com` |
| `:1458` | `isCloudflareAiGateway` | `cloudflare-ai-gateway`, `gateway.ai.cloudflare.com` |
| `:1459` | `isNvidia` | `nvidia`, `integrate.api.nvidia.com` |
| `:1460` | `isAntLing` | `ant-ling`, `api.ant-ling.com` |
| `:1461` | `isDeepSeek` | `deepseek`, lowercased URL contains `deepseek.com` |
| `:1463-1478` | `isNonStandard` | union of the above ∪ `cerebras.ai` ∪ `api.x.ai` ∪ `chutes.ai` ∪ `opencode.ai` |
| `:1480-1488` | `useMaxTokens` | `chutes.ai`, deepseek, moonshot, CF gateway, together, nvidia, ant-ling, zai |
| `:1490` | `isGrok` | `xai`, `api.x.ai` |
| `:1491-1492` | `isOpenRouterDeveloperRoleModel` | OpenRouter **and** model id starts `anthropic/` or `openai/` |
| `:1493` | `cacheControlFormat` | `"anthropic"` iff provider is `openrouter` and id starts `anthropic/` |

Resulting defaults (`:1495-1537`), the interesting ones:

```ts
supportsStore:        !isNonStandard                                             // :1496
supportsDeveloperRole: isOpenRouterDeveloperRoleModel || (!isNonStandard && !isOpenRouter)  // :1497
supportsReasoningEffort: !isGrok && !isZai && !isMoonshot && !isTogether
                       && !isCloudflareAiGateway && !isNvidia && !isAntLing      // :1498-1499
maxTokensField:       useMaxTokens ? "max_tokens" : "max_completion_tokens"      // :1502
thinkingFormat:       deepseek→"deepseek" | zai→"zai" | together→"together"
                    | antLing→"ant-ling" | openrouter→"openrouter" | "openai"    // :1507-1517
supportsStrictMode:   !isMoonshot && !isTogether && !isCloudflareAiGateway && !isNvidia  // :1524
sessionAffinityFormat: isOpenRouter ? "openrouter" : "openai"                    // :1529
supportsLongCacheRetention: !(isTogether || isCloudflareWorkersAI
                            || isCloudflareAiGateway || isNvidia || isAntLing)   // :1530-1536
```

**Documentation drift found:** `requiresToolResultName`, `requiresAssistantAfterToolResult`, and
`requiresThinkingAsText` are hard-coded `false` at `:1503-1505` and are **never auto-detected**,
despite `src/types.ts:558-563` claiming "Default: auto-detected from URL". They must be set
explicitly on `model.compat`. Similarly `src/types.ts:562` mentions `<thinking>` delimiters, but
`requiresThinkingAsText` emits plain text with **no** tags (`openai-completions.ts:1153-1158`).

**The responses adapter has almost no detection**: `getCompat()`
(`src/api/openai-responses.ts:67-78`) has exactly one URL rule — `detectSessionAffinityFormat()`
(`:49-51`, `openrouter.ai` → `"openrouter"`, else `"openai"`); everything else is a constant.

**The Anthropic adapter has no baseUrl detection at all.** `getAnthropicCompat(model)`
(`src/api/anthropic-messages.ts:173-186`) is pure `?? <constant>` defaults. Its one piece of
inference is `defaultSupportsToolReferences(model)` (`:193-200`), which requires
`provider === "anthropic"`, excludes any id containing `haiku`, and regex-parses the version from
`^claude-(opus|sonnet|fable)-(\d+)(-(\d+))?` requiring ≥ 4.5. Everything else is set at catalog
generation time (`scripts/generate-models.ts:947-955`).

### 3.3 The `sk-ant-oat` sniff (a notable design smell)

Anthropic's OAuth mode is not signaled through the auth layer. `toAuth()` returns a plain
`{apiKey: credential.access}` (`src/auth/oauth/anthropic.ts:361-363`), and the adapter
**sniffs the key string**: `isOAuthToken = (apiKey) => apiKey.includes("sk-ant-oat")`
(`src/api/anthropic-messages.ts:843-845`). When true it switches to `authToken` (Bearer), forces
`anthropic-beta: claude-code-20250219,oauth-2025-04-20,...`, `user-agent: claude-cli/2.1.75`,
`x-app: cli` (`:902-904`), **and rewrites tool names to Claude Code canonical names throughout the
request** (`:948, 976, 1095, 1217, 1315`). This is behavior keyed off a credential's textual
prefix, three layers below where the credential type is actually known.

### 3.4 Auth as the escape hatch: `ModelAuth`

The auth layer's whole vocabulary is three fields (`src/auth/types.ts:7-11`), with an explicit
doctrine at `:3-6`:

> If a value cannot be expressed as `apiKey`, `headers`, or `baseUrl`, it is provider config, not auth.

```ts
export interface ModelAuth { apiKey?: string; headers?: ProviderHeaders; baseUrl?: string; }
```

`ProviderHeaders = Record<string, string | null>` (`src/types.ts:110`) — **`null` deletes a
header**, which is how Cloudflare AI Gateway suppresses SDK-set `Authorization`/`x-api-key`.

Resolution order (`resolveProviderAuth`, `src/auth/resolve.ts:63-110`):

1. `overrides.env` overlays the AuthContext, scoped env beats process env (`:71`, `:112-117`).
2. **Explicit `options.apiKey` wins** — stored credentials are never read (`:73-85`).
3. **Stored credential** (`:87-102`): oauth → `resolveStoredOAuth`; api_key → `apiKey.resolve(credential)`.
   A type mismatch (stored oauth, no oauth handler) **returns `undefined` — no env fallback** (`:103`).
4. **Ambient/env only if nothing stored** (`:106-109`).

Doctrine at `:44-49`: *"A stored credential owns the provider… No silent env fallback after a
failed refresh."*

OAuth refresh is double-checked-locked inside `credentials.modify` (`:143-159`) with a 5-minute
validity margin and 15s timeout (`:119-120`), so concurrent processes cannot double-refresh.

Request-time application, `Models.applyAuth` (`src/models.ts:636-665`):

```
provider auth headers -> model.headers -> explicit options.headers -> transformHeaders -> Provider.stream*()
```

and critically `const requestModel = auth.baseUrl ? { ...model, baseUrl: auth.baseUrl } : model`
(`:659`) — **auth can rewrite the endpoint**. GitHub Copilot uses exactly this: `toAuth` returns
`{apiKey, baseUrl}` where the baseUrl is parsed out of the `proxy-ep=` claim inside the token
(`src/auth/oauth/github-copilot.ts:69-87, 410-416`).

### 3.4.1 The two reusable auth builders

`src/auth/helpers.ts` has only two exports, and they carry almost the whole catalogue:

- **`envApiKeyAuth(name, envVars)`** (`:9-31`) — secret-prompt login, resolve preferring
  `credential.key` (`:20-22`) then walking `envVars` in order (`:23-27`). Used by ~35 providers.
- **`lazyOAuth({name, isSubscription?, loginLabel?, load})`** (`:40-59`) — memoizes `load()` so a
  provider can *advertise* OAuth (name/label are static, needed for menus) without importing
  Node-only flow code.

Env-var table: `src/env-api-keys.ts:79-116`, with special cases first at `:68-77`
(github-copilot → `COPILOT_GITHUB_TOKEN`; anthropic → `[ANTHROPIC_AUTH_TOKEN, ANTHROPIC_OAUTH_TOKEN,
ANTHROPIC_API_KEY]`). Note this module is a **parallel, string-based view** used for status/discovery
only; the `ProviderAuth.resolve` path is what actually feeds requests.

### 3.5 Bedrock — special-cased, but *inside* the config pattern

`src/providers/amazon-bedrock.ts` is 90 lines, and its `auth` is still a normal `ProviderAuth`
(`:86`: `auth: { apiKey: bedrockAuth }`, no `oauth`). But the `ApiKeyAuth` is a **facade over the
AWS credential chain**:

- `login` (`:13-53`) offers a 3-way select: bearer-token / aws-profile / credential-chain.
  aws-profile returns a credential with **no key**, only `env: { AWS_PROFILE }` (`:41-46`);
  credential-chain returns `{type:"api_key"}` with neither (`:52`).
- `resolve` (`:54-79`) returns `{ auth: {} }` — an **empty ModelAuth** — for every non-bearer path,
  purely as a "yes, configured" signal: probing `AWS_BEARER_TOKEN_BEDROCK` (`:64`), `AWS_PROFILE`
  (`:65-71`), access-key pair (`:72-74`), ECS task roles (`:75-76`), web identity (`:77`).

So the framework passes `apiKey: undefined` + `env`, and **the real credential resolution happens
inside the adapter** (`src/api/bedrock-converse-stream.ts:140-221`), configuring
`BedrockRuntimeClientConfig` and letting the AWS SDK's default chain + SigV4 do the work. pi never
signs anything itself.

Bedrock specifics worth noting:

- **Custom headers go through a Smithy `build`-step middleware** (`:445-459`), registered at
  `:458` with `{step:"build", name:"pi-ai-custom-headers", priority:"low"}` — the `build` step runs
  after serialization but **before signing**, so injected headers are covered by SigV4. Reserved
  headers (`x-amz-*`, `authorization`, `host`) are silently dropped (`:431-436`). This contract is
  documented on `ProviderRequestOptions.headers` (`src/types.ts:147-154`).
- **Bearer token bypasses SigV4** entirely: `config.token = {token}` +
  `authSchemePreference = ["httpBearerAuth"]` (`:218-221`).
- **Region resolution** (`:171-183`), in priority order: region embedded in an inference-profile
  ARN (`:174-176`) > `options.region`/`AWS_REGION`/`AWS_DEFAULT_REGION` > region parsed out of a
  standard endpoint host > `us-east-1`, but only when no ambient `AWS_PROFILE`.
- **Cross-region prefixes** (`us.`/`eu.`/`apac.`) are passed through verbatim as `modelId`
  (`:237`); the *base URL* is chosen at generation time by
  `getBedrockBaseUrl` (`scripts/generate-models.ts:958-962`) which handles `eu.` and defaults
  everything else to `us-east-1` — **there is no `apac.` case**, it relies on runtime override.
- **Application inference profile ARNs don't contain the model name**, so every capability probe
  matches against **both `model.id` and `model.name`** via `getModelMatchCandidates` (`:638-644`),
  with `AWS_BEDROCK_FORCE_CACHE=1` as the last-resort override (`:744`).

### 3.6 Vertex — special-cased in three places

1. `googleVertexProvider()` (`src/providers/google-vertex.ts:92-100`) is the only Google provider
   with **no `baseUrl`**; the catalog baseUrl is the template
   `https://{location}-aiplatform.googleapis.com` (`scripts/generate-models.ts:208`), and
   `resolveCustomBaseUrl()` (`src/api/google-vertex.ts:399-405`) returns `undefined` while the URL
   still contains `{location}` so the SDK builds the regional endpoint itself.
2. Hand-written `ApiKeyAuth` (`src/providers/google-vertex.ts:13-90`) with a 3-way login
   (api-key / ADC / service-account) returning `env: {GOOGLE_CLOUD_PROJECT, GOOGLE_CLOUD_LOCATION,
   GOOGLE_APPLICATION_CREDENTIALS}` (`:55-62`); `resolve` returns `{auth: {}}` (no key) when ADC
   files + project + location exist (`:81-87`).
3. `src/env-api-keys.ts:155-166` synthesizes an `"<authenticated>"` sentinel for status UIs — and
   the adapter has to *defend against it*: `isPlaceholderApiKey()`
   (`src/api/google-vertex.ts:429-431`) rejects `<...>`-shaped keys so the sentinel doesn't get
   sent as a real credential.

There is **no OAuth module for Vertex**; token minting is entirely inside
`@google/genai` + google-auth-library.

### 3.7 Azure — URL construction

Azure is the one adapter that genuinely rewrites the URL rather than delegating.
`normalizeAzureBaseUrl()` (`src/api/azure-openai-responses.ts:181-210`) forces
`pathname = "/openai/v1"` for `*.openai.azure.com`, `*.cognitiveservices.azure.com`, and
`*.ai.azure.com` when the path is empty/`/openai`/`/openai/v1/responses` (`:190-207`).
`buildDefaultBaseUrl()` (`:212-214`) builds `https://<resource>.openai.azure.com/openai/v1`.
`resolveAzureConfig()` (`:216-249`) resolves in order: `azureBaseUrl` option → `AZURE_OPENAI_BASE_URL`
→ `azureResourceName`/`AZURE_OPENAI_RESOURCE_NAME` → `model.baseUrl`, else throw. The SDK then
appends `/deployments/<deployment>/responses?api-version=<v>`; `apiVersion` defaults to `"v1"`
(`:23`, `:220-223`). Deployment-name mapping via `AZURE_OPENAI_DEPLOYMENT_NAME_MAP` (`:41-49`).

### 3.8 OAuth

Seven flows in `src/auth/oauth/`, registered by `load.ts:14-22`: anthropic, openai-codex,
github-copilot, openrouter, xai, kimi-coding, radius.

| flow | style | file |
|---|---|---|
| Anthropic | PKCE + loopback (port 53692) raced against manual paste | `anthropic.ts:234-312` |
| OpenAI Codex | user picks browser-PKCE-loopback (port 1455) or device code | `openai-codex.ts:515-544` |
| GitHub Copilot | RFC 8628 device code + token exchange | `github-copilot.ts:357-395` |
| OpenRouter | PKCE, ephemeral port, exchanges code for a **permanent API key** | `openrouter.ts:242-299` |
| xAI | device code | `xai.ts:201-211` |
| Kimi Code | device code (JSON, not form) | `kimi-coding.ts:281-293` |
| Radius | select: browser PKCE (endpoint discovered at `/v1/oauth`) or device code | `radius.ts:357-384` |

Shared machinery: `pkce.ts:21-34` (Web Crypto, S256), `device-code.ts:46-98` (RFC 8628 poller with
`slow_down` handling and server-`interval` preference to survive VM clock drift, `:78-86`),
`oauth-page.ts:94-109` (HTML result pages).

`toAuth()` produces three distinct shapes: `{apiKey}` (anthropic, codex, openrouter, xai, radius),
`{headers: {Authorization: Bearer}}` (kimi — requires Bearer, not `x-api-key`,
`kimi-coding.ts:307-309`), and `{apiKey, baseUrl}` (github-copilot, `:410-416`).

**OpenRouter has no refresh**: `async refresh(credential) { return credential; }`
(`openrouter.ts:305-307`) because the exchange yields a permanent key stored as
`{access: body.key, refresh: "", expires: Number.MAX_SAFE_INTEGER}` (`:127-132`).

**Copilot's "refresh" is a token exchange, not an OAuth refresh** — the stored `refresh` is a
long-lived GitHub OAuth token, and refresh GETs `/copilot_internal/v2/token`
(`github-copilot.ts:253-288`), also re-fetching `availableModelIds` (`:293-303`) which the provider
feeds into `filterModels` (`src/providers/github-copilot.ts:19-27`).

Pluggability: `importOAuthModule()` (`load.ts:9-12`) uses a **variable specifier** so bundlers can't
follow it into `node:http`/`node:crypto`; `registerBundledOAuthFlowLoaders()` (`:27-29`) is the
static-registration path used by `src/bun-oauth.ts:11-21` for compiled binaries.

### 3.9 `compat.ts` and `legacy-api-aliases.ts`

Both are **deprecation shims**, not architecture.

`src/compat.ts:1-11` says so explicitly: *"Temporary compatibility entrypoint preserving the old
global pi-ai API surface… This module is deleted with the coding-agent ModelManager migration."*
It re-exports everything (`:13-29`), maintains a **module-level api-registry**
(`apiProviderRegistry`, `:100`; `registerApiProvider` `:126-138`) that is the *pre-collection*
design, registers the ten builtins at import time (`:178-213`), and provides global
`stream`/`complete`/`streamSimple` (`:250-298`) that inject env API keys via `withEnvApiKey`
(`:222-230`) — the old implicit-auth model, versus the new explicit `Models`+`CredentialStore`.

Note the direction of travel: the **registry pattern moved from a global singleton
(`compat.ts:100`) to an injected collection (`createModels()`, `src/models.ts:735-737`)**, while
the *images* side still uses the global-registry pattern (`src/images-api-registry.ts:24`).

`src/legacy-api-aliases.ts` is 109 lines of pure `@deprecated` aliases: `streamAnthropic`,
`streamOpenAICompletions`, `streamGoogle`, etc. (`:28-108`), each pointing at
`<api>Api().stream`. Zero logic.

---

## 4. Streaming — the unified event model

### 4.1 The event union

`AssistantMessageEvent` — `src/types.ts:523-539`:

```ts
| { type: "start";          partial: AssistantMessage }
| { type: "text_start";     contentIndex: number; partial }
| { type: "text_delta";     contentIndex: number; delta: string; partial }
| { type: "text_end";       contentIndex: number; content: string; partial }
| { type: "thinking_start"; contentIndex: number; partial }
| { type: "thinking_delta"; contentIndex: number; delta: string; partial }
| { type: "thinking_end";   contentIndex: number; content: string; partial }
| { type: "toolcall_start"; contentIndex: number; partial }
| { type: "toolcall_delta"; contentIndex: number; delta: string; partial }
| { type: "toolcall_end";   contentIndex: number; toolCall: ToolCall; partial }
| { type: "done";  reason: "stop"|"length"|"toolUse"|"deferred"; message: AssistantMessage }
| { type: "error"; reason: "aborted"|"error";                     error: AssistantMessage }
```

Every event carries a `partial: AssistantMessage` snapshot, so a consumer never has to accumulate
state itself. The canonical ordering is spelled out executably in the test double
`src/providers/faux.ts:338-434`.

### 4.2 The stream primitive

`AssistantMessageEventStream` (`src/utils/event-stream.ts:69-83`) extends a generic
`EventStream<T, R>` (`:4-67`) that is an **async-iterable + promise hybrid**:

- `push(event)` (`:21-36`) — no-ops once done (`:22`); if `isComplete(event)` it sets `done` and
  resolves the final-result promise **before** delivery (`:24-27`); then hands to a waiting
  consumer or queues (`:30-35`).
- `[Symbol.asyncIterator]` (`:50-62`) — drains queue, then parks a resolver.
- **`result(): Promise<R>`** (`:64-66`) just returns the stored promise. It does **not** drive
  iteration, so it can be awaited without consuming the stream, and consuming is not required for
  it to settle.
- Completion predicate (`:72`): `event.type === "done" || event.type === "error"`; extraction
  (`:74-78`) returns `event.message` or `event.error`. **Errors resolve the promise, never reject
  it** — the whole error model.

### 4.3 Per-adapter SSE decode → unified events

**openai-completions** (`:441-590`). SSE framing is inside the `openai` SDK; this loop is
chunk-level.
- `start` pushed once *before* the loop (`:256`).
- Text: `choice.delta.content` → `ensureTextBlock()` (`:351-358`, emits `text_start` on first use) →
  `text_delta` (`:481-486`).
- Thinking: probes `["reasoning_content", "reasoning", "reasoning_text"]`, **first non-empty wins**
  to avoid chutes.ai duplication (`:493-502`); the winning *field name* is stored as the block
  signature and replayed back as that literal key (`:1169-1176`), remapped `reasoning` →
  `reasoning_content` for `opencode-go` (`:507-511`).
- **Partial tool JSON**: accumulated in a scratch field `block.partialArgs` (`:537`) and re-parsed
  on **every** delta with `parseStreamingJson(block.partialArgs)` (`:538`). That helper
  (`src/utils/json-parse.ts:104-124`) tries `JSON.parse` → a hand-rolled `repairJson` (`:32-83`) →
  the `partial-json` package → `{}`. `toolcall_delta` carries the **raw** fragment (`:543-548`).
- `finishBlock()` (`:305-350`) closes each block and **deletes the scratch fields**
  `partialArgs`/`customInput`/`streamIndex` (`:340-342`) so replayed history carries only parsed args.
- OpenRouter encrypted reasoning: `delta.reasoning_details` of type `reasoning.encrypted` get
  JSON-stringified onto the matching tool call's `thoughtSignature`, parked in
  `pendingReasoningDetailsByToolCallId` if the tool call hasn't arrived yet (`:552-565`, applied
  `:371-380`).
- Terminal (`:572-587`): abort check; infer `stop`/`toolUse` when `!supportsFinishReason` (`:579-581`);
  throw `"Stream ended without finish_reason"` (`:585-587`).

**openai-responses** (`src/api/openai-responses-shared.ts:432-760`). Slot-based on `output_index`
(`:440`, `createSlot()` `:463-529`). Tool-call ids are composite: `` `${call_id}|${item.id}` ``
(`:485-503`). `reasoning_summary_part.done` injects `"\n\n"` (`:612-621`).
`function_call_arguments.done` replaces the buffer and emits only the **suffix** delta (`:658-668`).
`thinkingSignature = JSON.stringify(item)` (`:689`), `textSignature = encodeTextSignatureV1(...)`
(`:700`, `:49-53`).

**anthropic-messages** (`:573-745`) — the only adapter that **parses SSE by hand**
(`iterateSseMessages` `:387-444`, `decodeSseLine` `:332-356`, event allowlist `:307-314`), on top of
an SDK-issued request (`client.messages.create(..., {stream:true}).asResponse()`, `:560`).

| Anthropic event | lines | → unified |
|---|---|---|
| `message_start` | `:574-586` | no event; seeds usage incl. **`cacheWrite1h`** (`:582`), `calculateCost` (`:586`) |
| `content_block_start` text/thinking/redacted_thinking/tool_use | `:588-627` | `text_start` / `thinking_start` / `thinking_start`(redacted) / `toolcall_start` |
| `text_delta` / `thinking_delta` / `input_json_delta` | `:630-666` | `text_delta` / `thinking_delta` / `toolcall_delta` |
| **`signature_delta`** | `:667-674` | **appends to `thinkingSignature`, emits NO unified event** |
| `content_block_stop` | `:675-706` | `text_end` / `thinking_end` / `toolcall_end` |
| `message_delta` | `:707-744` | `rawStopReason` + `mapStopReason`; `output_tokens_details.thinking_tokens` → `usage.reasoning` (`:734-738`) |

Redacted thinking becomes `{type:"thinking", thinking:"[Reasoning redacted]", thinkingSignature: data,
redacted:true}` (`:605-614`). `mapStopReason` (`:1326-1352`) **throws** on an unknown reason
(`:1350`) — caught by the outer catch.

**bedrock-converse-stream** (`:264-296`) consumes the AWS SDK's async iterable; no SSE.
Notably **Bedrock sends no start events for text/thinking blocks**, so the adapter lazily
synthesizes `text_start`/`thinking_start` on the first delta (`:544-551`, `:564-570`).
`mapStopReason` (`:1034-1049`) does **not** throw on unknown (contrast Anthropic). `metadata` →
usage (`:590-603`) has **no `cacheWrite1h`, no `reasoning` tokens, no `responseId`**.

**google-generative-ai / google-vertex** (`:98-243` / `:116-260`). Only `candidates[0]` is consumed
(`:102`) — extra candidates are dropped. Thinking is `part.thought === true`
(`google-shared.ts:35-37`); `thoughtSignature` can ride on *any* part and does not imply thinking
(protocol notes `:20-34`). Because Gemini streams **complete** function calls rather than JSON
fragments, the adapter fabricates the whole triple in one shot: `toolcall_start` → a single
`toolcall_delta` carrying `JSON.stringify(arguments)` → `toolcall_end` (`:203-210`). Missing/duplicate
call ids are synthesized as `${name}_${Date.now()}_${++counter}` (`:186-192`).
`finishReason` → `mapStopReason` (`google-shared.ts:346-373`) then **overridden to `"toolUse"` if any
toolCall block exists** (`:215-221`).

**mistral-conversations** (`:553-746`). `delta.content` may be a string **or** an array of chunks;
`{type:"thinking", thinking:[{text}]}` → thinking events (`:639-660`). All `toolcall_end` events are
**deferred to end-of-stream** (`:731-745`). Tool-call ids must be exactly 9 alphanumerics
(`:223-253`).

### 4.4 Cross-adapter message normalization

`src/api/transform-messages.ts:64-223` is shared pre-flight normalization every adapter runs:

- Null/undefined `content` normalized to `[]` (`:73`).
- Images downgraded to text placeholders for non-vision models (`:35-56`).
- **Cross-model thinking handling** (`:100-117`): redacted thinking is dropped for a different
  model (opaque encrypted payload); same-model thinking with a signature is kept even when the text
  is empty (OpenAI encrypted reasoning); otherwise thinking becomes plain text.
- `thoughtSignature` stripped cross-model (`:131-134`); tool-call ids normalized via an injected
  `normalizeToolCallId` (`:136-142`) — OpenAI Responses ids are 450+ chars with `|`, Anthropic
  requires `^[a-zA-Z0-9_-]{1,64}$` (`:59-63`).
- **Errored/aborted assistant messages are skipped entirely on replay** (`:189-197`).
- **Synthetic tool results are injected for orphaned tool calls** (`:163-180`, `:220`) —
  `"No result provided"`, `isError: true` — because most APIs reject a tool call without a result.

### 4.5 Cloudflare — two transports for the same adapters

Cloudflare is **not** a distinct API implementation. Templates in `src/api/cloudflare.ts:2-15`
contain `{CLOUDFLARE_ACCOUNT_ID}` / `{CLOUDFLARE_GATEWAY_ID}` placeholders;
`resolveCloudflareModel()` (`src/providers/cloudflare-stream.ts:6-15`) substitutes them from
`options.env`, and `cloudflareStreams()` (`:21-27`) is a thin `ProviderStreams` **decorator**
applied per-API (`src/providers/cloudflare-ai-gateway.ts:17-21`). This decorator pattern is the
documented way to do provider-wide endpoint transformation (README §Custom Providers).

The second transport is genuinely interesting: `createGatewayBindingFetch()`
(`src/api/cloudflare-gateway-binding.ts:79-143`) returns a **`FetchFunction` shim** that translates
POSTs under the gateway prefix into `env.AI.gateway(id).run({provider, endpoint, headers, query})`
(`:141`). Rationale at `:1-21`: a Worker in the gateway's own account otherwise needs a Cloudflare
API token; binding calls are pre-authenticated and return the provider's native wire format as a
normal streaming `Response`, "so API implementations behave identically over either transport."
A sentinel `cf-aig-authorization: Bearer cloudflare-gateway-binding` (`:55`) satisfies adapters'
pre-dispatch auth checks and is stripped before the wire (`:71`).

---

## 5. Images / media

### 5.1 Image *input* (in chat)

Just a content-block variant: `ImageContent {type:"image", data: string /*base64*/, mimeType}`
(`src/types.ts:354-358`), allowed in `UserMessage.content` (`:411`) and `ToolResultMessage.content`
(`:441`) but **not** in `AssistantMessage.content` (`:417`) — assistants can't emit images on the
chat path. Capability is `Model.input` (`:806`), and unsupported images are silently replaced with
placeholders (§4.4).

Per-adapter encoding: OpenRouter/OpenAI → `data:${mimeType};base64,${data}` URLs; Bedrock → **raw
bytes**, not base64 (`bedrock-converse-stream.ts:1161-1188`); Google → `inlineData` parts
(`google-shared.ts:108-132`), with multimodal `functionResponse` parts only for Gemini ≥ 3
(`:200-223`).

### 5.2 Image *generation* — a fully separate, parallel API family

Confirmed: it mirrors the chat side **structurally, member for member**, with a completely separate
type hierarchy.

| chat | images |
|---|---|
| `Api` / `KnownApi` (10) | `ImagesApi` / `KnownImagesApi` (**1**) — `src/types.ts:31-33` |
| `Model<TApi>` | `ImagesModel<TApi>` — `src/types.ts:825-830` |
| `Context` | `ImagesContext {input: (Text|Image)[]}` — `:460-462` |
| `AssistantMessage` | `AssistantImages` — `:466-476` |
| `ProviderStreams` | `ProviderImages` — `:285-291` |
| `Provider` | `ImagesProvider` — `src/images-models.ts:12-43` |
| `Models` | `ImagesModels` — `src/images-models.ts:49-88` |
| `createProvider` | `createImagesProvider` — `:251-275` |
| `createModels` | `createImagesModels` — `:227-229` |
| `builtinModels()` | `builtinImagesModels()` — `src/providers/all.ts:149-155` |
| `MODELS` | `IMAGE_MODELS` — `src/image-models.generated.ts:6` |

`ImagesModel` = `Omit<Model, "api"|"provider"|"reasoning"|"contextWindow"|"maxTokens"|"compat">`
plus `output: ("text"|"image")[]` (`src/types.ts:825-830`). It keeps `cost`, `input`, `baseUrl`,
`headers`, `samplingParams`.

`ImagesStopReason = "stop" | "error" | "aborted"` (`:464`) — **no `length`, no `toolUse`**; image
models don't tool-call.

**It is one-shot, not streaming**: `ImagesFunction` returns `Promise<AssistantImages>`
(`:326-330`), `stream: false` in the request (`openrouter-images.ts:160`).
`AssistantMessageEventStream` is entirely absent from this path.

Registration is via the *older* global-registry pattern: `registerImagesApiProvider()`
(`src/images-api-registry.ts:38-49`) writing into a module-level `Map` (`:24`), with
`src/providers/images/register-builtins.ts:50` self-invoking at import time and `src/images.ts:1`
importing it for the side effect. Declared in `package.json:9-13` `sideEffects`.

### 5.3 There is no OpenAI images adapter

Verified by exhaustive grep across `src/`, `scripts/`, `test/`:

- `"images/generations"` → **zero matches**
- `"dall-e"` / `"dalle"` → **zero matches**
- `"gpt-image"` → matches **only** as OpenRouter catalog ids
  (`src/image-models.generated.ts:293, 308, 323`: `openai/gpt-image-1`, `-1-mini`, `-2`), all with
  `api: "openrouter-images"`.

`KnownImagesApi` has exactly one member, `register-builtins.ts:43-48` registers exactly one, and
`src/providers/images/` contains only `register-builtins.ts`. **Image generation reaches OpenAI
models only by proxy through OpenRouter.**

### 5.4 The `openrouter-images` adapter

`src/api/openrouter-images.ts` — and the surprise is that **it is not an images endpoint at all**.
It calls `client.chat.completions.create(...)` (`:72-73`) with `baseURL: model.baseUrl` (`:125`),
i.e. `POST https://openrouter.ai/api/v1/chat/completions`.

- **Request** (`buildParams`, `:136-163`): a single `role:"user"` message; text parts sanitized
  (`:141`); image parts inlined as `image_url` data URLs (`:147`); `stream: false` (`:160`); and the
  key field `modalities: model.output.includes("text") ? ["image","text"] : ["image"]` (`:161`) —
  **`ImagesModel.output` is load-bearing here**.
- **Response** (`:83-107`): images read from the **non-standard `choice.message.images[]`** array
  (typed `:24-38`). Only `data:` URLs are accepted — `if (!imageUrl?.startsWith("data:")) continue;`
  (`:98`), parsed by `/^data:([^;]+);base64,(.+)$/` (`:99`). **Remote http(s) image URLs are
  silently dropped**; there is no fetch-and-encode path.
- **Usage/cost** (`:165-196`): token-based only, via the same `$/M` rates.

### 5.5 The image catalog

`src/image-models.generated.ts` — 42 entries, all under `openrouter`. Entry shape (`:8-22`):
`{id, name, api:"openrouter-images", provider:"openrouter", baseUrl, input, output, cost}`.

**There is no per-image price primitive.** `ModelCost` is $/M-tokens only (`src/types.ts:776-791`),
and `scripts/generate-image-models.ts:77-82` maps OpenRouter's `prompt`/`completion` prices into it
— which is why many image entries land at all-zero cost (e.g. `black-forest-labs/flux.2-flex`,
`:16-21`), while `openai/gpt-image-1` gets `input: 10, output: 10, cacheRead: 1.25` (`:302-307`).

Source: `scripts/generate-image-models.ts:95` fetches
`https://openrouter.ai/api/v1/models?output_modalities=image`, filters to text|image modalities
(`:51-64`), skips anything whose output lacks `"image"` (`:66`), and hardcodes
`provider: "openrouter"` / `api: "openrouter-images"` (`:72-74`).

---

## 6. Errors, retries, usage accounting

### 6.1 Errors — normalized, and never thrown

Every adapter has one terminal `catch` doing the same four things
(`openai-completions.ts:591-611`, `openai-responses.ts:180-191`, `anthropic-messages.ts:760-770`,
`azure-openai-responses.ts:144-155`, `google-generative-ai.ts:279-290`,
`mistral-conversations.ts:163-166`):

1. strip streaming scratch fields (`partialArgs`, `index`, `customInput`) from partial blocks;
2. `stopReason = signal.aborted ? "aborted" : "error"`;
3. `errorMessage = formatProviderError(normalizeProviderError(error))`;
4. push a single `{type:"error", reason, error: output}` and end the stream.

`src/utils/error-body.ts` is the normalizer, and it exists for a specific reason stated at `:1-14`:
proxies return non-2xx bodies the SDKs don't fold into `error.message`, producing opaque strings
like `"403 status code (no body)"`.

- `extractStatus` (`:61-67`) probes **in order**: `statusCode` (Mistral) → `status` (openai,
  @google/genai) → `$metadata.httpStatusCode` (Bedrock) → `$response.statusCode`.
- `extractBody` (`:76-92`) probes `body` → `error` → `$response.body`, truncated to 4000 chars
  (`:16`, `:137-140`).
- **The redaction trick** is `isPlainNonEmptyObject` (`:112-117`): only objects whose prototype is
  `Object.prototype`/`null` count as bodies. The comment at `:98-111` explains the bug — AWS SDK
  v3's `$response.body` is a stream wrapper whose stringification produced `{"_events":...}` garbage
  that then *replaced* the real message.
- `messageCarriesBody` (`:46`) prevents double-printing.
- `formatProviderError` (`:128-135`) composes `"<status>: <body>"` or `"<prefix> (<status>): <body>"`.

Structured diagnostics ride alongside: `AssistantMessageDiagnostic {type, timestamp, error?, details?}`
(`src/utils/diagnostics.ts:8-13`), surfaced on `AssistantMessage.diagnostics` (`src/types.ts:423`).

**Context-overflow detection** is regex-based and remarkably empirical:
`src/utils/overflow.ts:37-63` holds **26 patterns** with a per-provider example table at `:3-36`,
plus `NON_OVERFLOW_PATTERNS` (`:74-78`) so Bedrock's `"ThrottlingException: Too many tokens"`
doesn't false-positive. `isContextOverflow` (`:134-163`) also catches two *silent* cases:
z.ai-style (`stopReason:"stop"` but `input + cacheRead > contextWindow`, `:145-150`) and
Xiaomi-style (`stopReason:"length"`, `output === 0`, input ≥ 99% of window, `:155-160`).

### 6.2 Retries — two layers

**Layer A — `src/utils/provider-retry.ts`, per-HTTP-request, inside adapters.**
- Retryable (`:23-35`): `x-should-retry` header forces either way; undefined status → retry;
  otherwise **408, 409, 429, ≥500**.
- Delay (`:51-67`): `retry-after-ms` → `retry-after` (seconds or HTTP-date) → jittered exponential
  `min(0.5 · 2^i, 8)s × (1 − random·0.25)`.
- **`maxRetryDelayMs` (default 60s) applies only to *server-requested* delays** (`:37-49`) and on
  breach **throws immediately** with `"Server requested Ns retry delay (max: Ms)"` rather than
  sleeping. `0` disables the cap.
- **Default `maxRetries` is `0`** (`:109`).
- Why it exists at all (`:97-104`): the SDKs' internal retry timers **ignore the request
  `AbortSignal`**, so every adapter passes `maxRetries: 0` to its SDK
  (`openai-completions.ts:245`, `openai-responses.ts:148`, `anthropic-messages.ts:557`) and wraps
  the call here (`:247-254`, `:150-157`, `:559-566`, `google-shared.ts:398-419`,
  `openrouter-images.ts:70`). Bedrock is the exception — it delegates retries to the AWS SDK.

**Layer B — `src/utils/retry.ts`, whole-turn, exported publicly (`src/index.ts:43`).**
`retryAssistantCall(produce, policy, signal, callbacks)` (`:163-212`) re-runs an entire assistant
turn. Classification is **string/regex over `errorMessage`**, not status codes:
`isRetryableAssistantError` (`:223-228`) rejects `NON_RETRYABLE_PROVIDER_LIMIT_ERROR_PATTERN`
(quota/billing, `:7-24`) then requires `RETRYABLE_PROVIDER_ERROR_PATTERN` (`:26-90`, covering
overload text, network failures, premature stream endings, WebSocket close, and `"retry delay"` —
i.e. the message thrown by layer A). Backoff `baseDelayMs · 2^(n−1)`, **no jitter, no cap** (`:196`).
**No adapter calls it**; it is for the agent host layer (`:94-96`). Bedrock even has a comment at
`bedrock-converse-stream.ts:400` about keeping `errorMessage` byte-identical so this regex still
matches — a fragile coupling worth noting.

### 6.3 Usage accounting

`Usage` — `src/types.ts:370-391`:

```ts
{ input, output, cacheRead, cacheWrite, cacheWrite1h?, reasoning?, totalTokens,
  cost: { input, output, cacheRead, cacheWrite, total } }
```

with two documented subtleties: `cacheWrite1h` is *"Only Anthropic reports this split"* (`:376`)
and `reasoning` is *"a subset of `output`: `output` already includes these tokens"* (`:377-382`).

Normalization is by convention, not by shared code — each adapter has its own parser, and they
agree on the **invariant that `input` excludes cache tokens**:

| adapter | site | `input` derivation |
|---|---|---|
| openai-completions | `:1375-1411` | `prompt_tokens − cacheRead − cacheWrite`; `cacheRead` from `prompt_tokens_details.cached_tokens ?? prompt_cache_hit_tokens` |
| openai-responses | `-shared.ts:559-575` | same shape |
| anthropic | `:578-585`, `:718-739` | native fields; `cacheWrite1h` from `cache_creation.ephemeral_1h_input_tokens` |
| google | `:223-242` | `promptTokenCount − cachedContentTokenCount`; `cacheWrite = 0` (no metric) |
| bedrock | `handleMetadata :590-603` | native; **no `cacheWrite1h`, no `reasoning`** |
| mistral | `:591-603` | `prompt_tokens − cachedPromptTokens`; **six** cached-token field spellings probed (`:532-551`) |

`calculateCost(model, usage)` (`src/models.ts:878-898`) is the one shared piece:

```ts
const inputTokens = usage.input + usage.cacheRead + usage.cacheWrite;   // tier selection basis
// highest matching tier wins  (:882-887)
const longWrite = usage.cacheWrite1h ?? 0;
const shortWrite = usage.cacheWrite - longWrite;
usage.cost.cacheWrite = (rates.cacheWrite * shortWrite + rates.input * 2 * longWrite) / 1e6;  // :895
```

The `rates.input * 2` is the hardcoded Anthropic 1h-cache-write rule (comment `:889`).
It **mutates `usage.cost` in place**. Post-hoc multipliers exist for OpenAI service tiers
(`openai-responses.ts:345-372`).

**Known gap:** Anthropic's `message_delta` handler updates `cacheWrite` (`:728-730`) but never
refreshes `cacheWrite1h`, so a mid-stream cache-write update can mis-price.

`src/utils/estimate.ts` provides the pre-flight estimate (4 chars/token, `:14`; images 4800 chars,
`:15`) and anchors to real reported usage where possible (`getLastAssistantUsageInfo` `:63-87`,
which guards against a compaction summary inserted after an older response, `:71-72`).

---

## 7. Verdict on the framing

> *"they just have `openai.responses()` adapters and things, and the client is mostly just config"*

**Confirmed on structure; refuted on where the complexity lives.**

Confirmed:
- 10 adapters, 40 providers, uniform 2-function contract, providers built by a declarative record.
- 24/40 providers are pure config (id, name, baseUrl, env var, catalog, adapter).
- Adding an OpenAI-compatible endpoint genuinely is ~15 lines (`createProvider` + `envApiKeyAuth`).

Refuted / qualified:
- **The per-provider difference did not vanish; it was relocated and typed.** ~30 `compat` flags
  (`src/types.ts:545-687`) plus a 95-line `baseUrl` substring-matching detector
  (`openai-completions.ts:1444-1538`). `thinkingFormat` alone has 11 variants.
- **9.5:1 code ratio** — 11,381 lines of adapter vs ~1,200 lines of provider config.
- **Three providers break the config pattern** with hand-written `ApiKeyAuth` facades over ambient
  credential systems (bedrock/AWS chain, vertex/ADC, cloudflare), and one adds a `ProviderStreams`
  decorator for URL templating.
- **Adapters are not thin.** Each carries message conversion, tool conversion, thinking/signature
  replay across model boundaries, prompt-cache marker placement, partial-JSON repair, usage
  normalization, and error-body archaeology.

### What is worth stealing

1. **`ProviderStreams` as the only seam.** Two functions, `(model, context, options) → EventStream`.
   Everything else (auth, catalog, dispatch, retry) composes around it.
2. **`ModelAuth = {apiKey?, headers?, baseUrl?}`** with the doctrine *"if it can't be expressed as
   these three, it's provider config, not auth"* (`src/auth/types.ts:3-11`). Combined with
   `Record<string, string|null>` headers where `null` means *delete*.
3. **Errors as stream events, never throws** (`src/types.ts:312-319`) — `result()` resolves with an
   error-carrying message rather than rejecting (`event-stream.ts:74-78`). One code path.
4. **`partial: AssistantMessage` on every event** — consumers never accumulate state.
5. **Capabilities as data on the model, generated from a shared registry** (models.dev), so adding
   a model is a data refresh rather than a code change.
6. **`stream` vs `streamSimple`** — the portable-vs-native option split, instead of pretending one
   abstraction covers both.
7. **`transform-messages.ts` as an explicit cross-model replay layer** — the thinking-signature /
   tool-id / orphaned-tool-call problems are real and worth solving once, centrally.
8. **The `compat` struct itself** — if you must have quirks, having them typed, per-API-family,
   defaulted-but-overridable, and documented in one interface beats `if (provider === "x")`
   scattered through a codebase.

### What to avoid

1. **`baseUrl` substring matching as capability inference** (`:1444-1493`). A user pointing
   `openrouter` at a proxy silently loses its quirk handling. Capabilities should be declared on the
   model/provider, not sniffed from a URL.
2. **Credential sniffing** — `apiKey.includes("sk-ant-oat")` (`anthropic-messages.ts:843-845`)
   deciding auth scheme, beta headers, *and* tool-name rewriting.
3. **`provider` id as behavior key inside adapters** (`opencode-go`, `github-copilot`, `xai` special
   cases in `openai-completions.ts`) — the adapter should not know provider ids.
4. **Regex-over-`errorMessage` as the retry/overflow classifier** (`retry.ts:26-90`,
   `overflow.ts:37-63`), with a comment in the Bedrock adapter (`:400`) warning you not to change a
   message string because a regex elsewhere depends on it.
5. **Documentation drift in the flags** — three `compat` fields document "auto-detected from URL"
   but are hardcoded `false` (`:1503-1505`).
6. **Duplicated adapters** — `google-generative-ai.ts` and `google-vertex.ts` are near-identical
   (`:98-243` vs `:116-260`), with `getGoogleBudget` duplicated and the Vertex copy already missing a
   `2.5-flash-lite` branch.
7. **Declared-but-unimplemented capability** — `fetchDeferred`/`cancelDeferred` exist through four
   layers with only a test double behind them.

---

## Appendix — file index

| concern | file |
|---|---|
| all core types, event union, compat structs | `src/types.ts` (830) |
| `Provider`/`Models`/`createProvider`/`createModels`/`calculateCost` | `src/models.ts` (944) |
| adapters | `src/api/*.ts` (11,381 total) |
| provider config records | `src/providers/*.ts` (~1,200 excl. faux/all) |
| generated catalog aggregator | `src/models.generated.ts` |
| generated per-provider shards | `src/providers/<id>.models.ts` → `src/providers/data/<id>.json` (gitignored) |
| catalog generator (models.dev) | `scripts/generate-models.ts` (2,948) |
| auth types / resolution / helpers | `src/auth/{types,resolve,helpers,context,credential-store}.ts` |
| OAuth flows | `src/auth/oauth/*.ts` (7 flows + pkce/device-code/load/oauth-page) |
| env var table | `src/env-api-keys.ts` |
| stream primitive | `src/utils/event-stream.ts` |
| retry (2 layers) | `src/utils/provider-retry.ts`, `src/utils/retry.ts` |
| error normalization | `src/utils/error-body.ts`, `src/utils/diagnostics.ts` |
| overflow / estimation | `src/utils/overflow.ts`, `src/utils/estimate.ts` |
| tool-arg validation (TypeBox) | `src/utils/validation.ts` |
| cross-model message normalization | `src/api/transform-messages.ts` |
| images (parallel family) | `src/images*.ts`, `src/api/openrouter-images.ts`, `src/providers/images/` |
| deprecation shims | `src/compat.ts`, `src/legacy-api-aliases.ts` |
| test double / minimal contract spec | `src/providers/faux.ts` (708) |
