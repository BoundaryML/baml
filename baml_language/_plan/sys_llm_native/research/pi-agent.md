# pi-mono: how the AGENT layer consumes the LLM layer

Research notes. Source tree (read-only copy):
`/private/tmp/claude-501/.../scratchpad/pi-mono` — all paths below are relative to that root
unless noted. Packages of interest:

- `packages/ai` — the LLM client library (`@earendil-works/pi-ai`). ~50 providers, 10 wire APIs.
- `packages/agent` — the agent runtime (`@earendil-works/pi-agent-core`). Loop + `Agent` class + a
  newer `harness/` layer (mostly stubbed).
- `packages/coding-agent` — the real application (the `pi` CLI/TUI) that wires the two together.

Line counts for orientation:

| file | lines |
|---|---|
| `packages/agent/src/agent-loop.ts` | 796 |
| `packages/agent/src/agent.ts` | 592 |
| `packages/agent/src/types.ts` | 443 |
| `packages/agent/src/stream-fn.ts` | 20 |
| `packages/agent/src/harness/agent-harness.ts` | 508 (mostly `HarnessNotImplemented`) |
| `packages/ai/src/types.ts` | 830 |
| `packages/ai/src/models.ts` | 944 |
| `packages/coding-agent/src/core/agent-session.ts` | 3344 |

---

## 1. The boundary interface: agent loop ↔ LLM api

### 1.1 It is exactly one function

The agent's entire dependency on the LLM layer is a single function type, `StreamFn`
(`packages/agent/src/types.ts:28-32`):

```ts
export type StreamFn = (
	model: Model<Api>,
	context: Context,
	options?: SimpleStreamOptions,
) => AssistantMessageEventStream | Promise<AssistantMessageEventStream>;
```

That is literally the `stream(model, context, options)` shape. Everything provider-specific hides
behind (a) the `Model` object and (b) whatever closure the host installed as the stream function.

The contract is spelled out in the doc comment at `packages/agent/src/types.ts:18-27` and repeated
at `packages/ai/src/types.ts:312-324`:

- must **not throw / not reject** for request, model, or runtime failures;
- failures are encoded *in the stream* as protocol events plus a terminal `AssistantMessage` with
  `stopReason: "error" | "aborted"` and `errorMessage`.

This is the single most important design decision in the whole boundary: the agent loop never has a
try/catch around the LLM call. It reads `message.stopReason` (`agent-loop.ts:196`) and branches.

There is a global default-injection escape hatch, `setDefaultStreamFn` /`getDefaultStreamFn`
(`packages/agent/src/stream-fn.ts:11-20`), so `pi-agent-core` has *no* build dependency on the
provider catalog. The app installs it: `packages/coding-agent/src/core/sdk.ts:36`
(`setDefaultStreamFn(streamSimple)`).

### 1.2 What is in `Context`

`Context` is 3 fields (`packages/ai/src/types.ts:509-513`):

```ts
export interface Context {
	systemPrompt?: string;
	messages: Message[];
	tools?: Tool[];
}
```

`Message = UserMessage | AssistantMessage | ToolResultMessage` (`packages/ai/src/types.ts:455`).
No provider-specific fields anywhere in it. It is constructed fresh at each turn in
`streamAssistantResponse` (`packages/agent/src/agent-loop.ts:298-302`):

```ts
const llmContext: Context = {
	systemPrompt: context.systemPrompt,
	messages: llmMessages,
	tools: context.tools,
};
```

### 1.3 The AgentMessage → Message funnel

The agent works with `AgentMessage` throughout and converts to `Message[]` **only at the LLM call
boundary** — this is stated in the file header comment (`agent-loop.ts:1-4`).

`AgentMessage = Message | CustomAgentMessages[keyof CustomAgentMessages]`
(`packages/agent/src/types.ts:325`), where `CustomAgentMessages` is an empty interface apps extend
via TS declaration merging (`packages/agent/src/types.ts:316-318`). So apps can put arbitrary
UI-only message types (notifications, bash-execution records, compaction summaries) in the
transcript, and two required hooks squeeze them out:

- `transformContext(messages, signal): Promise<AgentMessage[]>` — AgentMessage→AgentMessage, for
  pruning/injection (`packages/agent/src/types.ts:200`), applied first
  (`agent-loop.ts:289-292`).
- `convertToLlm(messages): Message[]` — AgentMessage→Message, required, filters UI-only messages
  (`packages/agent/src/types.ts:178`), applied second (`agent-loop.ts:295`).

Default implementation is a 3-role filter (`packages/agent/src/agent.ts:33-37`).

Both hooks are contractually forbidden from throwing (`types.ts:159-161`, `types.ts:186-188`).

### 1.4 Options

`AgentLoopConfig extends SimpleStreamOptions` (`packages/agent/src/types.ts:149`) — i.e. the agent's
own config object *is* the provider options bag, and is passed through nearly verbatim
(`agent-loop.ts:308-312`):

```ts
const response = await streamFunction(config.model, llmContext, {
	...config,
	apiKey: resolvedApiKey,
	signal,
});
```

`SimpleStreamOptions` (`packages/ai/src/types.ts:304-310`) = `StreamOptions` + `reasoning?:
ThinkingLevel` + `deferred?` + `thinkingBudgets?`. `StreamOptions`
(`packages/ai/src/types.ts:175-219`) carries `temperature`, `samplingParams`, `maxTokens`,
`transport`, `cacheRetention`, `sessionId`, `metadata`, `timeoutMs`, `maxRetries`,
`maxRetryDelayMs`, plus lifecycle callbacks `onPayload` / `onResponse` and `headers` / `env` /
`fetch` (`packages/ai/src/types.ts:120-173`).

Note `reasoning` is a *portable* enum (`"minimal"|"low"|"medium"|"high"|"xhigh"|"max"`,
`packages/ai/src/types.ts:82`), mapped per-model via `Model.thinkingLevelMap`
(`packages/ai/src/types.ts:801-805`) and clamped by `clampThinkingLevel`
(`packages/ai/src/models.ts:913-932`). Token-budget providers get a shared translation in
`adjustMaxTokensForThinking` (`packages/ai/src/api/simple-options.ts:61-86`), with a common
`buildBaseOptions` (`simple-options.ts:21-52`) and `clampMaxTokensToContext`
(`simple-options.ts:15-19`) so `maxTokens` never overruns the remaining context window.

### 1.5 Event protocol back to the agent

`AssistantMessageEvent` (`packages/ai/src/types.ts:523-539`) is a 12-case union:
`start`, `{text,thinking,toolcall}_{start,delta,end}`, `done`, `error`. Every event carries a
`partial: AssistantMessage`, so the agent never has to accumulate deltas itself — it just replaces
the last message in the transcript (`agent-loop.ts:335-343`). This is why the agent loop's streaming
handler is ~55 lines total (`agent-loop.ts:314-371`).

The agent re-emits these upward as its own `AgentEvent` union
(`packages/agent/src/types.ts:428-443`): agent/turn/message/tool lifecycle events.

### 1.6 Where the LLM layer's own `Models` object sits

`Models` (`packages/ai/src/models.ts:156-223`) is a runtime registry of `Provider` objects that
resolves auth and *delegates* — it does not implement wire protocols. `Models.streamSimple`
(`models.ts:690-696`) is: look up provider by `model.provider`, `applyAuth`, call
`provider.streamSimple`. `applyAuth` (`models.ts:636-665`) merges auth headers, injects the api key,
lets the model override `baseUrl`, and runs a `transformHeaders` hook.

`Provider` (`models.ts:97-149`) owns: id/name/baseUrl/headers, `auth: ProviderAuth`, `getModels()`,
optional `refreshModels()` (dynamic catalogs), optional `filterModels()`, and `stream`/`streamSimple`
(+ optional deferred). `createProvider()` (`models.ts:762-862`) builds one from a config blob;
the `api` field is either one `ProviderStreams` impl or a map keyed by `model.api`
(`models.ts:752-753`, dispatch at `models.ts:779-792`).

So the layering is:

```
Agent loop  --StreamFn-->  ModelRuntime/Models  --provider.streamSimple-->  api/*.ts (wire protocol)
```

---

## 2. Tools: definition and result flow

### 2.1 Definition

Base type in the LLM layer (`packages/ai/src/types.ts:502-507`):

```ts
export interface Tool<TParameters extends TSchema = TSchema> {
	name: string;
	description: string;
	parameters: TParameters;          // TypeBox schema
	constrainedSampling?: false | ConstrainedSamplingConfig;
}
```

Schemas are **TypeBox** (`typebox` package), re-exported from `packages/ai/src/index.ts:1-2`.
`ConstrainedSamplingConfig` (`packages/ai/src/types.ts:492-500`) is either
`{type:"json_schema", strict:"prefer"|"require"}` or `{type:"grammar", variants: {openai_lark?,
openai_regex?}}` — a portable way to ask for structured decoding.

The agent extends it (`packages/agent/src/types.ts:386-409`) with `label` (UI), optional
`prepareArguments` (pre-validation coercion shim), `executionMode?: "sequential"|"parallel"`, and:

```ts
execute(toolCallId, params, signal?, onUpdate?): Promise<AgentToolResult<TDetails>>
```

`AgentToolResult<T>` (`packages/agent/src/types.ts:361-375`): `content: (TextContent |
ImageContent)[]`, `details: T` (arbitrary, for UI/logs, never sent to the model), optional `usage`,
optional `addedToolNames`, optional `terminate`.

Concrete example — the bash tool (`packages/agent/src/harness/tools/bash.ts:11-14, 49-58`) is a
plain object literal with a TypeBox `Type.Object({command, timeout})`.

A third layer, `AgentHarnessTool` (`packages/agent/src/harness/types.ts:81-94`), adds a 5th
`context` parameter to `execute` so tools can be written once against an injected `ExecutionEnv`
(FileSystem + Shell abstraction, `harness/types.ts:231-315`) — which is how the same `read`/`bash`
tools work locally or against a remote sandbox.

### 2.2 Argument validation

`validateToolArguments(tool, toolCall)` from the AI package
(`packages/ai/src/utils/validation.ts`, imported at `agent-loop.ts:11`) is called in
`prepareToolCall` (`agent-loop.ts:618`), inside a try/catch that turns any failure into an error
tool result (`agent-loop.ts:661-667`). It uses TypeBox `Compile` with a `WeakMap` validator cache
(`validation.ts:1-6`) and does light primitive coercion (`validation.ts:57+`) because models emit
`"5"` for numbers.

### 2.3 Result flow back into the next turn

The loop is (`agent-loop.ts:203-224`):

1. `message.content.filter(c => c.type === "toolCall")`.
2. Execute → `ToolResultMessage[]`.
3. Push each result into `currentContext.messages` **and** `newMessages`.
4. `hasMoreToolCalls = !batch.terminate`, so the while loop runs another turn.

Correlation is by `toolCallId`. `createToolResultMessage`
(`agent-loop.ts:777-791`) builds:

```ts
{ role: "toolResult", toolCallId, toolName, content: result.content ?? [],
  details, usage, addedToolNames?, isError, timestamp }
```

Note it carries **both** `toolCallId` and `toolName` — the name is needed because several
OpenAI-compatible backends require `name` on tool result messages
(`OpenAICompletionsCompat.requiresToolResultName`, `packages/ai/src/types.ts:558-559`).

`ToolResultMessage` is a *top-level message role*, not a content block inside a user message
(`packages/ai/src/types.ts:437-453`). Each API adapter re-packs it into whatever the wire format
wants.

### 2.4 Execution modes & hooks

- Default `"parallel"` (`packages/agent/src/agent.ts:237`); one `sequential` tool in the batch
  forces the whole batch sequential (`agent-loop.ts:419-425`).
- Parallel mode preserves determinism carefully: preparation is sequential, execution concurrent,
  `tool_execution_end` fires in *completion* order, but tool-result **messages** are emitted in
  assistant *source* order (`agent-loop.ts:489-554`, documented at `types.ts:36-41`).
- `beforeToolCall` can block with a reason (`agent-loop.ts:619-647`); `afterToolCall` can rewrite
  content/details/isError/usage/terminate field-by-field (`agent-loop.ts:724-751`).
- `terminate` only stops the loop when **every** result in the batch sets it
  (`shouldTerminateToolBatch`, `agent-loop.ts:582-584`).
- Streaming partial tool output: `onUpdate(partialResult)` → `tool_execution_update` events, with
  a guard that drops callbacks fired after the promise settles (`agent-loop.ts:675-711`).

### 2.5 Dynamically-added tools

`AgentToolResult.addedToolNames` (`packages/agent/src/types.ts:368-369`) propagates onto the
`ToolResultMessage` (`agent-loop.ts:787`) and is documented in the LLM layer at
`packages/ai/src/types.ts:445-450`: "Providers with native deferred tool loading use this as the
load point; other providers ignore it and use `Context.tools` normally." (See
`packages/ai/src/utils/deferred-tools.ts`, `OpenAIResponsesCompat.supportsToolSearch` /
`AnthropicMessagesCompat.supportsToolReferences`.)

---

## 3. Cross-cutting behaviours

### 3.1 Error classification and retries

Two levels, deliberately separated.

**(a) Inside the provider SDK** — `maxRetries` / `maxRetryDelayMs` in `ProviderRequestOptions`
(`packages/ai/src/types.ts:160-172`). If the server asks for a delay longer than
`maxRetryDelayMs` (default 60s) the request fails immediately so *higher-level* logic can surface it
to the user.

**(b) In the agent/app** — `packages/ai/src/utils/retry.ts`. This is the classifier + policy:

- `RetryPolicy = {enabled, maxRetries, baseDelayMs}` (`retry.ts:98-104`), backoff is
  `baseDelayMs * 2^(attempt-1)` (`retry.ts:196`).
- `isRetryableAssistantError(message)` (`retry.ts:223-228`) is a **regex over
  `errorMessage` text**, with a deny-list first: `NON_RETRYABLE_PROVIDER_LIMIT_ERROR_PATTERN`
  (`retry.ts:7-24`, quota/billing/subscription limits) then
  `RETRYABLE_PROVIDER_ERROR_PATTERN` (`retry.ts:26-90`, ~45 patterns covering overload/429/5xx,
  network/proxy/DNS, websocket close, premature stream end, gRPC `ResourceExhausted`).
  Each pattern is annotated with the provider and GitHub issue that motivated it.
- `retryAssistantCall(produce, policy, signal, callbacks)` (`retry.ts:163-212`) is a bounded retry
  wrapper with `onRetryScheduled` / `onRetryAttemptStart` / `onRetryFinished` callbacks for UI, and
  it normalizes an abort-during-backoff into an `aborted` AssistantMessage.

This is honest but crude: **classification is string matching on provider error text, not typed
error codes.** The 90-line regex list is the price of a normalized error surface across 50 providers.

The app layer (`coding-agent`) implements its own copy of the same policy for the *agent turn*
(rather than reusing `retryAssistantCall`, which it uses only for compaction/summarization calls,
`agent-session.ts:2136`):

- `_isRetryableError` = "not context overflow, and `isRetryableAssistantError`"
  (`agent-session.ts:2645-2649`).
- `_prepareRetry` (`agent-session.ts:2686-2736`): increments attempt, computes
  `baseDelayMs * 2^(n-1)`, emits `auto_retry_start`, **pops the failed assistant message off
  `agent.state.messages`** (keeping it in the persisted session for history,
  `agent-session.ts:2710-2714`), sleeps abortably, then the caller calls `agent.continue()`.

### 3.2 Context overflow (separate channel from retry)

`packages/ai/src/utils/overflow.ts` — `isContextOverflow(message, contextWindow)`
(`overflow.ts:134-163`) detects three cases:

1. error-text pattern match — 26 regexes (`overflow.ts:37-63`) each documented with the exact
   provider string it came from (`overflow.ts:9-36`), minus a `NON_OVERFLOW_PATTERNS` deny-list for
   throttling messages that mention "too many tokens" (`overflow.ts:74-78`);
2. **silent** overflow (z.ai returns 200) — detected via `usage.input + usage.cacheRead >
   contextWindow` (`overflow.ts:145-150`);
3. **length-stop** overflow (Xiaomi MiMo truncates input to exactly fill the window, then returns
   `length` with `output === 0`) — `overflow.ts:155-160`.

`isRecoverableLength` (`overflow.ts:171-173`): a `length` stop whose output came in *below* the
originally desired cap, i.e. probably context pressure rather than a genuinely long answer.

The doc comment at `overflow.ts:119-128` is telling: for custom providers added via settings.json,
overflow detection *does not work* and the user is told to submit a regex.

The app routes overflow to compaction, not retry (`agent-session.ts:1962-2053`):
- `sameModel` guard so an overflow error from the *previous* model doesn't trigger compaction after
  a model switch (`agent-session.ts:1971-1976`);
- a compaction-boundary timestamp guard so stale pre-compaction usage doesn't re-trigger
  (`agent-session.ts:1978-1986`);
- one-shot compact-and-retry, tracked by `_overflowRecoveryAttempted`
  (`agent-session.ts:2001-2021`), which pops the failed assistant message before retrying.

### 3.3 `stopReason` handling

`StopReason = "pending"|"stop"|"length"|"toolUse"|"error"|"aborted"|"deferred"`
(`packages/ai/src/types.ts:393`).

The loop's treatment (`agent-loop.ts:196-214`):
- `error` / `aborted` → emit `turn_end` + `agent_end`, return immediately.
- `length` **with tool calls** → do *not* execute them. `failToolCallsFromTruncatedMessage`
  (`agent-loop.ts:381-406`) fails every call in the batch with an explanatory message telling the
  model to re-issue with complete arguments. Rationale at `agent-loop.ts:374-380`: the streaming
  JSON salvage parser will happily produce *valid-looking but silently truncated* arguments. This is
  a genuinely subtle bug class and they handled it.

### 3.4 Usage accumulation

`Usage` (`packages/ai/src/types.ts:370-391`) is per-message, normalized:
`input/output/cacheRead/cacheWrite/cacheWrite1h?/reasoning?/totalTokens` plus a nested
`cost:{input,output,cacheRead,cacheWrite,total}`.

The agent loop **does not accumulate usage at all**. Each `AssistantMessage` carries its own
`usage`, and aggregation is the app's job:

- `calculateCost(model, usage)` (`packages/ai/src/models.ts:878-898`) mutates `usage.cost` in place;
  handles tiered pricing (`ModelCostTier.inputTokensAbove`, `types.ts:783-791`, highest matching
  threshold wins for the whole request) and Anthropic's 2×-base-input charge for 1h cache writes
  (`models.ts:889-895`).
- App-side rollup: `addUsageToTotals` (`packages/coding-agent/src/core/usage-totals.ts:22-28`) and
  `getUsageCostBreakdown` (`usage-totals.ts:37-70`), which buckets by
  `` `${provider}/${responseModel ?? model}` `` — note it prefers `responseModel`, the *concrete*
  model a router like OpenRouter actually used (`packages/ai/src/types.ts:421`).
- The newer harness persists usage as its own session record type and accumulates in a reducer
  (`packages/agent/src/harness/session/state.ts:143-147`).
- Tool results can carry their own `usage` (sub-agent calls), explicitly excluded from main-context
  accounting (`packages/agent/src/types.ts:366`, `packages/ai/src/types.ts:443-444`), and bucketed
  as `"Tools/summaries"` (`usage-totals.ts:46-52`).

### 3.5 Thinking / reasoning persistence across turns

`ThinkingContent` (`packages/ai/src/types.ts:344-352`) has three fields: `thinking` text,
`thinkingSignature` (opaque provider token — OpenAI reasoning item id, Anthropic signature), and
`redacted?`. `ToolCall` additionally has a Google-specific `thoughtSignature`
(`types.ts:365`) and an OpenAI-Responses `namespace` (`types.ts:367`).

Persistence is handled entirely in the AI layer, in **`packages/ai/src/api/transform-messages.ts`**
— see next section.

### 3.6 Cross-provider handoff (the interesting bit)

There is no explicit "handoff" API. Instead, **every API adapter calls `transformMessages()` on the
way out**, and that function does per-message `isSameModel` reconciliation.

Call sites (one per wire protocol):
- `packages/ai/src/api/anthropic-messages.ts:947` (normalizer at `:1077`)
- `packages/ai/src/api/openai-completions.ts:1081` (normalizer at `:1055`)
- `packages/ai/src/api/openai-responses-shared.ts` (normalizer at `:158`)
- `packages/ai/src/api/google-shared.ts:105` (normalizer at `:100`)
- `packages/ai/src/api/bedrock-converse-stream.ts:839` (normalizer at `:790`)
- `packages/ai/src/api/mistral-conversations.ts:138`

`transformMessages(messages, model, normalizeToolCallId?)`
(`packages/ai/src/api/transform-messages.ts:64-223`) does, in order:

1. **Null-content normalization** for untyped callers / old session files (`:73`).
2. **Image downgrade** — if `!model.input.includes("image")`, replace every `ImageContent` in user
   and toolResult messages with a text placeholder, collapsing runs
   (`downgradeUnsupportedImages`, `:35-57`; placeholders at `:12-13`).
3. **Per-block reconciliation** keyed on
   `isSameModel = provider && api && model.id all equal` (`:95-98`):
   - `redacted` thinking → kept only for same model, otherwise **dropped** (`:103-106`);
   - thinking with a signature → kept for same model even if text is empty (OpenAI encrypted
     reasoning) (`:109`);
   - thinking for a *different* model → **converted to a plain text block** (`:112-116`);
   - text blocks for a different model → rebuilt without `textSignature` (`:119-125`);
   - `toolCall.thoughtSignature` → **stripped** cross-model (`:131-134`);
   - tool call ids → rewritten via the per-API `normalizeToolCallId` callback, and the mapping is
     replayed onto the matching `toolResult` messages (`:136-142`, applied at `:84-90`). Motivation
     in the comment at `:59-63`: OpenAI Responses ids are 450+ chars with `|`, Anthropic requires
     `^[a-zA-Z0-9_-]+$` ≤64.
4. **Second pass — orphan repair** (`:158-221`): drops assistant messages with
   `stopReason "error"|"aborted"` entirely (`:194-197`, because replaying a partial turn causes
   e.g. OpenAI "reasoning without following item"), and synthesizes
   `{content:"No result provided", isError:true}` tool results for any tool call left unanswered
   (`:163-180`), including at end-of-conversation.

The per-API `normalizeToolCallId` callbacks are the only per-provider message hooks in the system:

- Anthropic: `id.replace(/[^a-zA-Z0-9_-]/g, "_").slice(0, 64)` —
  `packages/ai/src/api/anthropic-messages.ts:1077-1079`.
- OpenAI Completions: splits the pipe-separated `{call_id}|{item_id}` ids emitted by
  Responses-family providers (github-copilot, openai-codex, opencode), recombines, hashes if >40
  chars — `packages/ai/src/api/openai-completions.ts:1055-1079`.
- OpenAI Responses: preserves the `call|item` pair but rebuilds a synthetic `fc_<hash>` item id when
  the source message came from a *foreign* provider/api —
  `packages/ai/src/api/openai-responses-shared.ts:153-170`.
- Google: only normalizes when `requiresToolCallId(model.id)` —
  `packages/ai/src/api/google-shared.ts:100-105`.

This is the whole cross-provider story, and it is ~220 lines of shared code. It means a user can
switch model mid-session and the next request is automatically sanitized. The app does almost
nothing extra on switch — `AgentSession.setModel` (`packages/coding-agent/src/core/agent-session.ts:1586-1601`)
just checks auth, sets `agent.state.model`, appends a `model_change` session entry, persists the
default, and re-clamps the thinking level. The only session-level awareness is the `sameModel` guard
in compaction (`agent-session.ts:1971-1976`).

Mid-run switching is supported by the loop itself: `prepareNextTurn` can return a new
`{context, model, thinkingLevel}` between turns (`agent-loop.ts:226-245`,
`AgentLoopTurnUpdate` at `packages/agent/src/types.ts:138-145`). The app uses this hook to
re-snapshot system prompt / tools / model / thinking level every turn
(`agent-session.ts:535-556`).

### 3.7 Compat surfaces (naming trap)

`packages/ai/src/compat.ts` (298 lines) and the `@earendil-works/pi-ai/compat` subpath export are
**not** message compatibility — they are the *legacy global API*, explicitly marked for deletion
(`packages/ai/src/compat.ts:1-11`: "Temporary compatibility entrypoint preserving the old global
pi-ai API surface … This module is deleted with the coding-agent ModelManager migration").
It contains: lazy-api re-exports (`:13-29`), deprecated static catalog accessors (`:62-69`), a
global api registry `Map<string, RegisteredApiProvider>` (`:100-158`), a `BUILTIN_APIS` side-effect
registration of all 10 apis (`:178-205`, executed at `:213`), env-key injection `withEnvApiKey()`
(`:222-230`) with an `AMBIENT_AUTH_MARKER = "<authenticated>"` sentinel (`:216`), and module-level
`stream`/`streamSimple` that dispatch on `model.api` (`:250-298`). `packages/ai/src/index.ts:4-8`
says the core index is side-effect free precisely to keep this out.
`packages/ai/src/compat/` has exactly one file, `extension-oauth-types.ts` (45 lines), unrelated to
messages. `coding-agent` still imports from the compat entrypoint
(`packages/coding-agent/src/core/sdk.ts:3`, `agent-session.ts:47`).
Per-*wire-format* compatibility flags live in `types.ts` as `OpenAICompletionsCompat`
(`packages/ai/src/types.ts:545-605`, ~30 flags), `OpenAIResponsesCompat` (`:608-625`),
`AnthropicMessagesCompat` (`:628-681`), `BedrockCompat` (`:684-687`), selected by a conditional type
on `Model.compat` (`:814-822`).

---

## 4. Cost of adding a provider: what is per-provider vs shared

### 4.1 The census

`packages/ai/src` is 23,056 lines. Of that:

- `packages/ai/src/api/` = 11,381 lines (49%) — **~10 wire protocols, all shared**.
- All 40 provider definition files together = 2,380 lines, and **708 of that is `faux.ts`**, the
  test double. So real provider definitions are ~1,670 lines for 40 providers.
- 39 generated `*.models.ts` files, **exactly 8 lines each**.

### 4.2 A new OpenAI-compatible provider is 15 lines

`packages/ai/src/providers/groq.ts` is the whole file:

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

`packages/ai/src/providers/cerebras.ts` (15), `deepseek.ts` (15), `together.ts` (15),
`huggingface.ts` (15), `baseten.ts` (15), `nvidia.ts` (15) are **byte-identical modulo 5 tokens**
(id, display name, baseUrl, env var, models constant).

`packages/ai/src/providers/xai.ts` is 28 lines — the extra 13 are an OAuth entry
(`lazyOAuth`, `xai.ts:14-18`) and a two-api map (`"openai-completions"` + `"openai-responses"`).
`openrouter.ts` is 23, `fireworks.ts` is 19 (two-api map, anthropic-messages + openai-completions).

So the marginal cost is: **15 lines + 8 generated lines + one line in
`packages/ai/src/providers/all.ts` + one entry in the `KnownProvider` union
(`packages/ai/src/types.ts:35-75`) + one entry in `packages/ai/src/env-api-keys.ts:79-116`.**

Registration: `builtinProviders()` at `packages/ai/src/providers/all.ts:89-132` returns an array of
40 factory calls; `builtinModels()` at `:135-141` registers them into a `Models`.

Each api also has a 4-line lazy shim so a provider can reference it without bundling the
implementation, e.g. `packages/ai/src/api/openai-completions.lazy.ts:4`:
`export const openAICompletionsApi = (): ProviderStreams => lazyApi(() => import("./openai-completions.ts"));`
(`lazyApi` at `packages/ai/src/api/lazy.ts:73`, `lazyStream` at `:46`).

### 4.3 The wire protocols (shared)

| api id | file | lines |
|---|---|---|
| `openai-codex-responses` | `packages/ai/src/api/openai-codex-responses.ts` | 1662 |
| `openai-completions` | `packages/ai/src/api/openai-completions.ts` | 1577 |
| `anthropic-messages` | `packages/ai/src/api/anthropic-messages.ts` | 1352 |
| `bedrock-converse-stream` | `packages/ai/src/api/bedrock-converse-stream.ts` | 1188 |
| `mistral-conversations` | `packages/ai/src/api/mistral-conversations.ts` | 931 |
| *(shared by 3 responses apis)* | `packages/ai/src/api/openai-responses-shared.ts` | 792 |
| `google-vertex` | `packages/ai/src/api/google-vertex.ts` | 596 |
| `google-generative-ai` | `packages/ai/src/api/google-generative-ai.ts` | 521 |
| `pi-messages` (in-house) | `packages/ai/src/api/pi-messages.ts` | 433 |
| *(shared by both google apis)* | `packages/ai/src/api/google-shared.ts` | 419 |
| `openai-responses` | `packages/ai/src/api/openai-responses.ts` | 372 |
| `azure-openai-responses` | `packages/ai/src/api/azure-openai-responses.ts` | 330 |

Plus shared helpers: `transform-messages.ts` (223), `constrained-sampling.ts` (277),
`simple-options.ts` (86), `lazy.ts` (98).

### 4.4 Model catalogs are generated from models.dev

`packages/ai/src/providers/groq.models.ts` in full:

```ts
// This file is auto-generated by scripts/generate-models.ts
// Do not edit manually - run 'npm run generate-models' to update
import values from "./data/groq.json" with { type: "json" };
import { flattenModelCatalog, type ModelCatalog } from "../model-catalog.ts";
export const GROQ_MODELS: ModelCatalog<typeof values, "groq"> = flattenModelCatalog("groq", values);
```

- Generator `packages/ai/scripts/generate-models.ts` is **2,948 lines**, run via
  `npm run generate-models` (`packages/ai/package.json:52`).
- Upstream is **models.dev**: `fetch("https://models.dev/api.json")` at
  `packages/ai/scripts/generate-models.ts:1315`.
- The JSON catalogs `packages/ai/src/providers/data/*.json` are **gitignored**
  (`packages/ai/.gitignore:11`) and hydrated at build time.
- Aggregate index `packages/ai/src/models.generated.ts` (124 lines, generated).

### 4.5 Where the per-provider cost actually hides

Three places, and they matter more than the 15-line files:

**(a) Quirk correction tables in the generator** — ~2,000 of the 2,948 lines of
`packages/ai/scripts/generate-models.ts` are per-provider fixups to models.dev data: thinking-level
maps, `compat` deltas, pricing tiers, model exclusion sets. E.g. `TOGETHER_*` at
`generate-models.ts:155-204`, `NVIDIA_*` at `:209-239`, `QWEN_TOKEN_PLAN_*` at `:274-328`,
`GITHUB_COPILOT_*` at `:444-465`, `OPENAI_*` at `:348-432`.

**(b) URL/provider-id sniffing inside the shared apis.** `detectCompat()` at
`packages/ai/src/api/openai-completions.ts:1444-1535` hard-codes z.ai, together, moonshot,
openrouter, cloudflare×2, nvidia, ant-ling, deepseek, cerebras, xai, chutes, opencode — deriving
`supportsStore`, `supportsDeveloperRole`, `supportsReasoningEffort`, `maxTokensField`,
`thinkingFormat`, `supportsStrictMode`, `sessionAffinityFormat`, `supportsLongCacheRetention`.
`getCompat()` at `:1541-1577` layers explicit `model.compat` over the detected defaults.
The vocabulary of quirks is the four `*Compat` interfaces in `packages/ai/src/types.ts:545-687`
(~30 flags for openai-completions alone). Some are provider-id equality checks in shared code, e.g.
`provider === "opencode-go"` at `openai-completions.ts:508` and `:1171`; github-copilot at
`openai-responses.ts:224`, `anthropic-messages.ts:525` and `:868`, `openai-completions.ts:647`.

**(c) OAuth flows** — `packages/ai/src/auth/oauth/` is ~2,900 lines for 7 providers:
`openai-codex.ts` 544, `github-copilot.ts` 417, `radius.ts` 403, `anthropic.ts` 364,
`openrouter.ts` 311, `kimi-coding.ts` 310, `xai.ts` 239, plus shared `device-code.ts` 98,
`oauth-page.ts` 109, `pkce.ts` 34, `load.ts` 68.

### 4.6 The ~8 providers that need real bespoke code

All of it is **auth resolution or endpoint materialization — never wire-protocol code**:

| provider | file | what's extra |
|---|---|---|
| amazon-bedrock | `packages/ai/src/providers/amazon-bedrock.ts:11-80` (90) | hand-written `ApiKeyAuth` with an interactive select (bearer / AWS profile / credential chain), probes 6 AWS env credential sources at `:64-77`; SigV4 signing lives in the 1188-line api |
| google-vertex | `packages/ai/src/providers/google-vertex.ts:13-90` (100) | api-key vs ADC vs service-account select; checks `~/.config/gcloud/application_default_credentials.json` + `GOOGLE_CLOUD_PROJECT` + `GOOGLE_CLOUD_LOCATION` (`:76-87`) |
| github-copilot | `packages/ai/src/providers/github-copilot.ts` (34) | `filterModels` gated on the OAuth credential's `availableModelIds` (`:19-27`), 3-api map, per-request header injection (`packages/ai/src/api/github-copilot-headers.ts:23-36`), per-credential baseUrl via `OAuthAuth.toAuth` |
| openai-codex | `packages/ai/src/providers/openai-codex.ts` (22) | OAuth-only (no `apiKey` method at all), base `https://chatgpt.com/backend-api`, its own 1662-line api |
| cloudflare ×2 | `cloudflare-workers-ai.ts` (15), `cloudflare-ai-gateway.ts` (23) | a `ProviderStreams` **decorator** `cloudflareStreams()` (`packages/ai/src/providers/cloudflare-stream.ts:6-27`) that materializes `{CLOUDFLARE_ACCOUNT_ID}` placeholders in `model.baseUrl` at dispatch; gateway auth nulls out `Authorization`/`x-api-key` (`cloudflare-auth.ts:91-97`) |
| anthropic | `packages/ai/src/providers/anthropic.ts:9-41` (59) | `ANTHROPIC_AUTH_TOKEN` becomes a raw `Authorization: Bearer` header, not an api key (`:24-31`); falls through to `ANTHROPIC_OAUTH_TOKEN` then `ANTHROPIC_API_KEY` |
| radius | `packages/ai/src/providers/radius.ts` (82) + `radius-config.ts` (96) | the only provider that **bypasses `createProvider`** and hand-writes the `Provider` object (`radius.ts:27-81`) — dynamic gateway-fetched catalog with legacy cache import |
| opencode / opencode-go | `opencode.ts` (24), `opencode-go.ts` (20) | no `baseUrl` at all (per-model baseUrls from catalog), 4- and 3-api maps |

### 4.7 They test cross-provider handoff explicitly

`packages/ai/test/cross-provider-handoff.test.ts` (522 lines) generates a
thinking+toolcall+toolresult fixture per provider, then feeds *every other provider's* messages into
each target and asserts no error. Strategy docblock at `:1-23`, driver at `:350-420`.



---

## 5. How the app configures models, providers, and keys

### 5.1 `ModelRuntime` — the app's own `Models`

`packages/coding-agent/src/core/model-runtime.ts:130` defines `ModelRuntime implements Models`
(787 lines). `ModelRuntime.create()` (`:172`) does:

1. wrap the credential store in `RuntimeCredentials` (`:173`);
2. load `~/.pi/agent/models.json` (`:174-176`, path from `packages/coding-agent/src/config.ts:529`);
3. pick a `ModelsStore` — `FileModelsStore` at `models-store.json`, else in-memory (`:177-181`);
4. take `builtinProviders()` and wrap every provider except `radius` with `withRemoteCatalog(...)`
   (`:182-189`);
5. `createModels({credentials, modelsStore})` (`:168`), then `rebuildProviders()` (`:199`).

`PI_OFFLINE` globally disables network (`:196`).

**Provider composition order**, `composeModelProvider()`
(`packages/coding-agent/src/core/provider-composer.ts:420`, applied in `getModels()` at `:433-446`):

```
builtin (or native extension provider)
  → models.json provider block   (baseUrl, compat, models[] UPSERT)   provider-composer.ts:168
  → extension registerProvider   (models[] REPLACE)                   provider-composer.ts:208
  → extension.oauth.modifyModels(models, credential)                  provider-composer.ts:439
  → models.json modelOverrides[id]  (topmost)                         provider-composer.ts:442-445
```

If neither a models.json block nor an extension overlay exists, the builtin provider is used
**untouched** to preserve its exact auth/stream behavior (`model-runtime.ts:253-257`).

A pi.dev remote catalog overlays every provider: `withRemoteCatalog`
(`packages/coding-agent/src/core/remote-catalog-provider.ts:45`) fetches
`https://pi.dev/api/models/providers/<id>` (`:80`) with ETag/If-None-Match revalidation and a 4-hour
freshness window (`:7`, `:70-75`), ignoring entries older than the bundled catalog's `generatedAt`
(`:36-42`).

### 5.2 `"anthropic/claude-sonnet-4"` → `Model`

`resolveCliModel()` (`packages/coding-agent/src/core/model-resolver.ts:405`) deliberately searches
**all** models, not just authenticated ones, so `--api-key` works on first run (`:417-419`). Order:

1. `--provider` must resolve or hard-error (`:435-441`).
2. Otherwise split on the **first** `/`; if the prefix is a known provider id, use it (`:451-462`).
   This is why `anthropic/claude-sonnet-4` resolves as provider+pattern rather than matching an
   OpenRouter model literally named `anthropic/...`.
3. Exact `id` or `provider/id` matches; on cross-provider ambiguity prefer the single
   *authenticated* one, else error (`:469-503`).
4. `parseModelPattern` (`:203`) recursively strips a trailing `:<thinkingLevel>` suffix
   (`:215-235`) — this is what makes both `sonnet:high` and `openrouter/model:exacto` work.
5. Fuzzy fallback: substring on `id` **or** `name`, preferring aliases (`-latest` or no `-YYYYMMDD`
   suffix) over dated ids (`:73-80`, `:142-164`).
6. **Unknown-model fallback**: with an explicit provider, `buildFallbackModel()` (`:174-188`) clones
   that provider's default model and swaps id/name, warning "Using custom model id" (`:592-593`).
   `--models` scope patterns additionally support globs via minimatch (`:291-336`).

**Startup precedence** (`packages/coding-agent/src/main.ts:470-520`, then
`packages/coding-agent/src/core/sdk.ts:194-217`, then
`findInitialModel()` at `model-resolver.ts:621`):

1. `--model` / `--provider`
2. saved default (`settings.defaultProvider`+`defaultModel`) if inside the scoped set
3. first scoped model from `--models` / `settings.enabledModels`
4. model recorded in the resumed session — only if it still exists **and** has configured auth
   (`sdk.ts:197-206`)
5. `settings.defaultProvider`/`defaultModel`, only if `hasConfiguredAuth` (`model-resolver.ts:670-680`)
6. first entry of the hardcoded `defaultModelPerProvider` map (`model-resolver.ts:20-61`) that is
   available
7. first available model at all

`--thinking` always beats any `:level` suffix (`main.ts:516-518`).

### 5.3 API keys: full precedence chain

The rule, stated at `packages/ai/src/auth/resolve.ts:44-49`: **a stored credential owns the
provider; ambient/env is consulted only when nothing is stored.** `resolveProviderAuth()`
(`resolve.ts:50-109`):

```
1. overrides.apiKey (request-level / --api-key)                  resolve.ts:73-85
2. stored credential from CredentialStore                        resolve.ts:87-104
     "oauth"   -> resolveStoredOAuth (locked refresh)             :89-98
     "api_key" -> apiKey.resolve(credential)                      :99-102
     type mismatch -> undefined (NO env fallback)                 :103
3. ambient: apiKey.resolve(undefined) -> env / AWS chain / ADC   resolve.ts:106-109
```

In the app the store is `RuntimeCredentials` over `AuthStorage`, so the effective chain is:

```
runtime key (--api-key)          runtime-credentials.ts:26-27
  -> ~/.pi/agent/auth.json       auth-storage.ts:442-448
  -> models.json `apiKey`        provider-composer.ts:349-354
  -> provider env vars / ambient env-api-keys.ts, auth/helpers.ts:23-28
```

**Env var map** is `getApiKeyEnvVars()` at `packages/ai/src/env-api-keys.ts:68-120` — a flat
`Record<providerId, envVarName>` (`:79-116`) plus two special cases: anthropic has a 3-var fallback
chain `ANTHROPIC_AUTH_TOKEN` → `ANTHROPIC_OAUTH_TOKEN` → `ANTHROPIC_API_KEY` (`:75-77`), where
`AUTH_TOKEN` is deliberately *skipped* by `getEnvApiKey` because it must be sent as
`Authorization: Bearer` (`:72-74`, `:149`); and github-copilot uses `COPILOT_GITHUB_TOKEN` (`:69-71`).
Ambient (non-key) sources return the sentinel string `"<authenticated>"`: google-vertex needs an ADC
file + `GOOGLE_CLOUD_PROJECT` + `GOOGLE_CLOUD_LOCATION` (`:154-165`); amazon-bedrock accepts any of
6 AWS credential mechanisms (`:167-185`).

`envApiKeyAuth(name, envVars)` (`packages/ai/src/auth/helpers.ts:9-30`) is the one-liner every
provider uses; it returns a `source` label ("stored credential" or the env var name) for status UI.

**Storage.** `AuthStorage` (`packages/coding-agent/src/core/auth-storage.ts:328`) backs
`~/.pi/agent/auth.json` with `0o600`/`0o700` perms (`:57`, `:63-65`, `:105-106`), cross-process
`proper-lockfile` locking (`:95-201`), a revision-keyed read cache (`:402-440`), and a single write
path `modify()` (`:450-472`). Notably `read()` resolves stored keys through `resolveConfigValue`
(`:445-448`), so even `auth.json` values may be `$ENV_VAR` or `!command`.

**Config-value indirection** (`packages/coding-agent/src/core/resolve-config-value.ts`) applies to
models.json `apiKey`/`headers` and auth.json keys: `"!cmd"` runs a shell command (stdout trimmed,
10s timeout, process-lifetime cached, `:145-151`, `:198-216`); `"$VAR"` / `"${VAR}"` interpolates
env (`:88-113`); `"$$"`/`"$!"` escape.

### 5.4 OAuth token refresh

`OAuthAuth` (`packages/ai/src/auth/types.ts:206-230`) splits into `login`, `refresh` (network,
throws), and `toAuth` (pure credential → `ModelAuth`) precisely so the `Models` layer can own the
locking.

`resolveStoredOAuth()` (`packages/ai/src/auth/resolve.ts:127-178`): minimum validity is
`max(5min, minOAuthValidityMs)` (`:119`, `:135`); an optimistic `expiresSoon` check is followed by
**double-checked locking inside `credentials.modify()`** (`:139-159`) — re-read under the lock, bail
if another process already refreshed, else `oauth.refresh(current, signal)` with a 15s timeout.
Failures wrap as `ModelsError("oauth", ...)` and **preserve the stored credential for retry**
(`:155-163`).

Two concrete flows worth noting:

- **Anthropic (Claude Pro/Max)** — `packages/ai/src/auth/oauth/anthropic.ts`: PKCE + localhost
  callback on `127.0.0.1:53692` (`:32-35`, `:99-168`), browser flow raced against a manual-paste
  prompt (`:264-301`), and `expires = now + expires_in*1000 - 5min` (`:230`, `:351`) — a built-in
  safety margin on top of the 5-minute `expiresSoon` window.
- **GitHub Copilot** — `packages/ai/src/auth/oauth/github-copilot.ts`: device-code flow
  (`:146-251`); the stored `refresh` token is the long-lived *GitHub* token, exchanged at
  `/copilot_internal/v2/token` for a short-lived Copilot token (`:253-288`). The refresh **also
  re-fetches the account's available model ids** (`:293-303`) which `filterModels` then applies
  (`packages/ai/src/providers/github-copilot.ts:19-27`), and `toAuth` derives a *per-credential*
  `baseUrl` by parsing `proxy-ep=` out of the token (`:69-87`, `:411-416`).

**Where `AgentLoopConfig.getApiKey` connects.** In `coding-agent` it is *not* used — the app's
`streamFn` closure calls `modelRuntime.streamSimple`, which resolves auth per request inside
`prepareRequest` (`packages/coding-agent/src/core/model-runtime.ts:573-608`). The agent-level
`getApiKey(provider)` hook (`packages/agent/src/types.ts:210`, applied at
`packages/agent/src/agent-loop.ts:305-306`) exists for hosts that want to hand a fresh token per
turn without a `Models` instance. Separately, `ExtensionOAuthConfig.getApiKey`
(`packages/coding-agent/src/core/provider-composer.ts:41`) is the *extension* API bridged to
pi-ai's `toAuth` at `provider-composer.ts:254`.

### 5.5 Yes — custom providers and models at runtime, no recompile

Three mechanisms:

**(a) `~/.pi/agent/models.json`** — the main one. Schema is TypeBox-compiled in
`packages/coding-agent/src/core/model-config.ts`; root is
`{ providers: Record<string, ProviderConfig> }` (`:207-209`). JSON-with-comments is supported
(`:262`), validation errors are reported per-path (`:270-277`), and it is **reloaded on every
`ModelRuntime.refresh()`** (`model-runtime.ts:691`) — which the `/model` picker triggers, so edits
take effect mid-session.

`ProviderConfigSchema` (`model-config.ts:194-205`):

| field | notes |
|---|---|
| `name`, `baseUrl`, `api`, `headers` | `baseUrl` required when defining custom models |
| `apiKey` | supports `!cmd` / `$VAR` indirection |
| `authHeader` | auto `Authorization: Bearer <apiKey>` (`provider-composer.ts:265-268`) |
| `oauth` | literal `"radius"` only — the one hardcoded OAuth escape (`model-config.ts:199`) |
| `compat` | union of the three `*Compat` shapes (`:136-140`) |
| `models[]` | `ModelDefinitionSchema` (`:157-171`), **upsert** by id |
| `modelOverrides` | `Record<modelId, ModelOverride>` (`:173-192`), topmost layer |

`ModelDefinitionSchema` fields: `id` (required), `name`, `api`, `baseUrl`, `reasoning`,
`thinkingLevelMap`, `input`, `cost` (+`tiers`), `contextWindow`, `maxTokens`, `samplingParams`,
`headers`, `compat`. Defaults applied in `modelFromJson()`
(`provider-composer.ts:130-166`): `contextWindow=128000`, `maxTokens=16384`, `input=["text"]`,
zero cost.

**This is the important part for a language design**: the full `compat` quirk vocabulary is exposed
in the user-editable config — `OpenAICompletionsCompatSchema` at `model-config.ts:71-110` mirrors
all ~30 flags including `thinkingFormat` (11 variants), `maxTokensField`,
`requiresThinkingAsText`, `chatTemplateKwargs`, `cacheControlFormat`, `openRouterRouting`. A user
can point at a random vLLM/llama.cpp/SGLang endpoint and describe its quirks declaratively, without
touching TypeScript.

**(b) Extension `pi.registerProvider(name, config)`** — `ProviderConfigInput`
(`provider-composer.ts:46-71`) adds `streamSimple` (a bespoke JS stream function!),
`oauth: ExtensionOAuthConfig`, and `refreshModels(ctx)`. Registered mid-session via
`ModelRuntime.registerProvider` (`model-runtime.ts:742`), which validates in isolation first so a
broken re-registration can't corrupt stored config (`:745`). Note extension `models[]` **replace**
rather than upsert (`provider-composer.ts:208-235`).

**(c) `--api-key` / `/login`** — auth only, via `setRuntimeApiKey` (`model-runtime.ts:536`).

**Settings.** There is **no JSON Schema file for settings.json** — the schema is the TypeScript
`Settings` interface at `packages/coding-agent/src/core/settings-manager.ts:90-140`. Project
`<cwd>/.pi/settings.json` deep-merges over global `~/.pi/agent/settings.json` (`:196-205`), and
project settings load only when the project is trusted (`:302-311`). `models.json` is a **separate
file**; custom providers never live in settings.json.

### 5.6 Model-switch UX

`/model` opens `ModelSelectorComponent`
(`packages/coding-agent/src/modes/interactive/components/model-selector.ts:36`), which renders the
cached `getAvailableSnapshot()` immediately and kicks off a background catalog refresh with a 15s
timeout, degrading to "showing cached models" on failure (`:163-201`). Concurrent refreshes are
deduped process-wide by `ModelCatalogRefreshCoordinator`
(`modes/interactive/model-catalog-refresh.ts:13-50`). It shows **only models from configured
providers** (`:103`).

On switch (`AgentSession.setModel`, `agent-session.ts:1586-1601`) **the conversation is untouched** —
no rewriting, no truncation, no re-validation against the new context window or image support.
What happens is: auth re-checked (throws on failure), thinking level re-clamped
(`_getThinkingLevelForModelSwitch` at `:1740-1748` → `clampThinkingLevel`), a `model_change` session
entry + global settings default persisted, and a `model_select` extension event.

The only user-visible compat feedback is (a) an Anthropic-subscription-billing warning
(`interactive-mode.ts:4693-4721`) and (b) a **cache-miss notice** on the next turn labelled
"Cache miss after model switch" (`interactive-mode.ts:3705-3727`,
`packages/coding-agent/src/core/cache-stats.ts:112-119` deliberately does *not* reset the cache
baseline on model switch, so the waste is attributed).

`cache-stats.ts` is worth noting on its own: `detectMiss()` (`:56-90`) computes
`missedTokens = min(prev.promptTokens, promptTokens) - cacheRead` and prices it at
`max(0, paidPerToken - readPerToken)`, i.e. it quantifies prompt-cache waste in dollars, with a
sticky `reportedCache` flag to distinguish "provider never reports caching" from "genuine total
miss" (`:44-47`, `:66-68`).

### 5.7 Attribution headers

`mergeProviderAttributionHeaders` (`packages/coding-agent/src/core/provider-attribution.ts:79-97`)
matches by provider id **or baseUrl hostname** (`:11-34`) so proxied providers still match. It adds
OpenRouter `HTTP-Referer`/`X-OpenRouter-Title` (`:44-50`), NVIDIA `X-BILLING-INVOKE-ORIGIN`
(`:52-56`), Cloudflare `User-Agent` (`:58-62`) — all gated on telemetry consent — and opencode
`x-opencode-session` (`:67-77`, not gated). Wired as the `transformHeaders` callback of the agent's
`streamFn` (`packages/coding-agent/src/core/sdk.ts:321-331`), applied last in `prepareRequest`
(`model-runtime.ts:592-593`).

## 6. Images and media in the agent loop

**There is no image generation in the agent loop.** The image API is a completely parallel stack in
`packages/ai` with no consumer in `packages/agent` or `packages/coding-agent`:

- `generateImages(model, context, options)` (`packages/ai/src/images.ts:14-21`), registry at
  `packages/ai/src/images-api-registry.ts`, one API impl
  (`packages/ai/src/api/openrouter-images.ts`, 196 lines), one provider
  (`packages/ai/src/providers/openrouter-images.ts`).
- Types: `ImagesModel` (`packages/ai/src/types.ts:825-830`), `ImagesContext`
  (`:460-462`), `AssistantImages` (`:466-476`), `ProviderImages` (`:285-291`).
- A grep for `generateImages` across `packages/` hits only `packages/ai/**` and its tests. The TUI,
  agent, and coding-agent never call it.

**Images flow only *into* the model**, via two paths, both as `ImageContent {type:"image", data:
base64, mimeType}` (`packages/ai/src/types.ts:354-358`):

1. **User messages** — `Agent.prompt(text, images?)` (`packages/agent/src/agent.ts:349`,
   normalization at `:402-406`). `UserMessage.content` is `string | (TextContent|ImageContent)[]`
   (`packages/ai/src/types.ts:411`).
2. **Tool results** — `AgentToolResult.content` is `(TextContent | ImageContent)[]`
   (`packages/agent/src/types.ts:363`). The `read` tool sniffs magic bytes
   (`detectSupportedImageMimeType`, `packages/agent/src/harness/tools/image.ts:3-10`, supporting
   jpeg/png/gif/webp/bmp with animated-PNG and CMYK-JPEG rejection) and returns a text label plus an
   image block (`packages/agent/src/harness/tools/read.ts:69-95`). An optional injected
   `imageProcessor` handles resize/convert (`read.ts:60-73`).

App-side normalization happens in `afterToolCall`: `normalizeToolResultImages(content,
{autoResizeImages})` (`packages/coding-agent/src/core/agent-session.ts:518-520`, helper in
`packages/coding-agent/src/utils/tool-result-images.ts`, resize workers in
`packages/coding-agent/src/utils/image-resize*.ts`). There is also a "block images" mode that strips
images from the LLM context in `convertToLlm` (`packages/coding-agent/src/core/sdk.ts:275-292`).

Downgrade for non-vision models is automatic and shared, in `transformMessages`
(§3.6 step 2) — the app never has to check `model.input`.

**Implication for a design that wants image *output*:** pi-mono's `AssistantMessage.content` is
`(TextContent | ThinkingContent | ToolCall)[]` (`packages/ai/src/types.ts:417`) — there is **no
ImageContent case**. A model that returns an image inline (Gemini image output, GPT image) has
nowhere to put it in their assistant message type. Image generation is modeled as a separate
non-conversational call returning `AssistantImages.output: (TextContent|ImageContent)[]`
(`:466-476`). If you want generated images to surface in an agent loop, the natural fit here would
be a *tool* that calls `generateImages` and returns `ImageContent` in its tool result — the plumbing
for that already exists end-to-end, but nobody has written that tool.

---

## 7. Judgment material

### 7.1 What makes this design cheap

1. **One function is the whole boundary.** `StreamFn = (model, context, options) =>
   AssistantMessageEventStream`. The agent package has zero build-time dependency on the provider
   catalog; the app injects the function (`packages/agent/src/stream-fn.ts:11`,
   `packages/coding-agent/src/core/sdk.ts:36`). Tests substitute a fake in one line.
2. **Errors are values, not exceptions.** The stream never rejects; failures arrive as
   `AssistantMessage{stopReason:"error"|"aborted", errorMessage}`
   (`packages/agent/src/types.ts:24-27`). The loop therefore has no error plumbing at all — it reads
   a discriminant (`agent-loop.ts:196`).
3. **Every streaming event carries a full `partial: AssistantMessage`**
   (`packages/ai/src/types.ts:523-539`), so consumers never accumulate deltas. The agent's entire
   streaming handler is 55 lines.
4. **Normalization happens once, in shared code.** `transformMessages`
   (`packages/ai/src/api/transform-messages.ts:64-223`) handles image downgrade, thinking-block
   portability, tool-id rewriting, orphan repair, and dropping failed turns — for all 10 wire
   protocols. The agent and the app never think about it.
5. **Quirks are declarative flags, not code paths.** `Model.compat` is a per-api conditional type
   (`packages/ai/src/types.ts:814-822`) with ~30 boolean/enum knobs for openai-completions alone
   (`:545-605`). A new OpenAI-compatible backend with a weird reasoning field is a `thinkingFormat`
   value, not a new adapter.
6. **Model catalogs are generated from an external source** (models.dev), so pricing/context/
   capability drift is a rebuild, not a PR.
7. **The quirk vocabulary is exposed to end users as data.** The full `*Compat` flag set is
   re-declared as a TypeBox schema in `packages/coding-agent/src/core/model-config.ts:71-140` and
   accepted from `~/.pi/agent/models.json`. A user can add a provider pi has never heard of — a
   vLLM/llama.cpp/SGLang endpoint with an unusual reasoning field — by writing JSON. That is only
   possible *because* the quirks were modeled as declarative flags rather than as code branches.

### 7.2 What is expensive / fragile

1. **Error classification is regex over provider error *text*.** `retry.ts:7-90` is ~85 lines of
   patterns; `overflow.ts:37-78` is another 40. Both are annotated with the provider and GitHub
   issue that motivated them, which is honest but tells you this list will never be finished.
   Custom providers get no overflow detection at all (`overflow.ts:119-128` tells users to submit a
   regex).
2. **`detectCompat()` sniffs baseUrl and provider id** (`openai-completions.ts:1444-1535`). There
   are also raw `provider === "..."` checks scattered in shared api files. This is the leak in the
   "providers are config" abstraction.
3. **~2,000 lines of correction tables inside the code generator**
   (`packages/ai/scripts/generate-models.ts`) — the real per-provider tax, invisible from the
   15-line provider files.
4. **OAuth is ~2,900 lines for 7 providers.** Subscription-based access (Copilot, Codex, SuperGrok,
   Claude Pro) is where the genuine per-provider engineering lives.
5. **The app re-implements the agent-turn retry loop** rather than using
   `retryAssistantCall` (compare `packages/coding-agent/src/core/agent-session.ts:2686-2736` with
   `packages/ai/src/utils/retry.ts:163-212`) — because agent-turn retry needs to mutate the
   transcript and interleave with compaction, which the generic helper can't do.
6. **`AssistantMessage.content` has no image case** (`packages/ai/src/types.ts:417`), so native
   image-output models don't fit the conversational type.
7. **The `harness/` layer is aspirational.** `packages/agent/src/harness/agent-harness.ts` defines a
   rich durable/lane/tree API (508 lines of types) but nearly every method returns
   `HarnessNotImplemented` (`:355-357`, `:363-442`). The working system is still `Agent` +
   `AgentSession`. Read `harness/` as a design direction, not shipped behavior.

### 7.3 The shape worth stealing

The layering that does the work:

```
Agent loop                     transcript, tools, steering, turn control — provider-agnostic
   |  StreamFn(model, Context, SimpleStreamOptions)
Models / ModelRuntime          registry + auth resolution + header merge; no wire code
   |  provider.streamSimple(model, context, options)
Provider (15-line config blob) id, baseUrl, auth method, model list, api selector
   |  ProviderStreams
api/*.ts (10 wire protocols)   transformMessages() -> serialize -> SSE parse -> AssistantMessageEvent
```

Four abstractions, each with one job. The `Model` object is the carrier for everything
provider-specific that the layers above need to know: `api`, `provider`, `baseUrl`, `reasoning`,
`thinkingLevelMap`, `input`, `cost`, `contextWindow`, `maxTokens`, `samplingParams`, `headers`,
`compat` (`packages/ai/src/types.ts:794-823`).
