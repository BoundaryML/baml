# 05 — Cross-cutting concerns: production concerns every serious app hits

> The capability files (`01`–`04`) describe what a single call can do. This file
> covers the concerns that wrap *every* call once an app leaves the prototype
> stage: talking to many providers at once, surviving failures, not paying for
> the same tokens twice, knowing what a call cost, running the code somewhere
> real, and finding out — before a 400 — whether a model can do what you asked.
> None of these are about a single request shape; they are about the system
> around the requests.

These are orthogonal to which provider you use, but each one bends differently
per provider. Single-turn mechanics are in `01-single-turn.md`; tool loops and
agents in `02-tools-and-agents.md`; sessions/memory in `03-state-sessions-memory.md`;
transports in `04-realtime-and-transports.md`.

Legend: ★ table-stakes · ◆ advanced · ▲ frontier.

---

## ★ Provider diversity & gateways

**Goal:** "I want to call many different models — possibly through a proxy or
gateway — without rewriting my code per provider, and switch models by
changing a string."

### How it's done today

There are dozens of inference providers, and they fall into three buckets:

1. **First-party APIs** with their own wire shape — OpenAI Chat Completions,
   OpenAI Responses, Anthropic Messages, Google Gemini `generateContent`. These
   diverge structurally (see `01`/`02`): system prompt as a message vs a
   top-level field; `tool` role vs tool-results-inside-user vs a `function`
   role; `response_format` vs synthetic `_return` tool vs `responseSchema`; SSE
   vs chunked-JSON streaming.
2. **OpenAI-compatible proxies** — vLLM, Ollama, Together, Groq, Fireworks,
   DeepSeek, OpenRouter, LM Studio, and most self-hosted servers expose the
   *Chat Completions* shape. You point the OpenAI SDK at a different `base_url`
   and pass a different key. This is the single most common interop strategy in
   the wild: "OpenAI-compatible" has become a de-facto wire standard.
3. **Universal bridges** — LiteLLM (Python) and the Vercel AI SDK (TypeScript)
   present *one* calling surface and translate to each provider's native wire
   format underneath.

The cheapest portability trick is **`base_url` + custom headers**: keep the SDK,
change the endpoint.

```python
# Python — point the OpenAI SDK at any OpenAI-compatible endpoint
from openai import OpenAI

groq = OpenAI(
    base_url="https://api.groq.com/openai/v1",
    api_key=os.environ["GROQ_API_KEY"],
    default_headers={"x-tenant": "team-a"},   # gateways often key off headers
)
resp = groq.chat.completions.create(
    model="llama-3.3-70b-versatile",
    messages=[{"role": "user", "content": "Hi"}],
)
```

```python
# Python — local Ollama, same SDK, different base_url
local = OpenAI(base_url="http://localhost:11434/v1", api_key="ollama")
local.chat.completions.create(model="qwen2.5", messages=[...])
```

**Model-string prefix routing** is how universal bridges pick a provider from a
single field. The prefix before the `/` selects the backend; the rest is the
model id.

```python
# Python — LiteLLM as a universal bridge; the prefix routes
import litellm

litellm.completion(model="openai/gpt-4.1",            messages=[...])
litellm.completion(model="anthropic/claude-sonnet-4-5", messages=[...])
litellm.completion(model="gemini/gemini-2.5-flash",   messages=[...])
litellm.completion(model="openrouter/meta-llama/llama-3.3-70b", messages=[...])
litellm.completion(model="bedrock/anthropic.claude-3-5-sonnet-20241022-v2:0", messages=[...])
# Returns an OpenAI-shaped response object regardless of backend.
```

LiteLLM normalizes *to* the OpenAI Chat Completions response shape, so callers
read `resp.choices[0].message.content` no matter the backend. It also ships a
proxy server (an OpenAI-compatible endpoint that fans out to many providers)
that adds key management, budgets, and routing as a deployable service.

In TypeScript the Vercel AI SDK uses a **factory + callable-namespace** pattern.
Each `@ai-sdk/*` package exports a default singleton plus a `create*` factory
that takes `apiKey` / `baseURL` / `headers`:

```ts
// TS — Vercel AI SDK: default singleton vs configured factory
import { generateText } from 'ai';
import { openai, createOpenAI } from '@ai-sdk/openai';
import { anthropic } from '@ai-sdk/anthropic';

// default singleton — reads OPENAI_API_KEY from env
await generateText({ model: openai('gpt-4.1'), prompt: 'Hi' });

// a custom gateway with overridden base URL + auth header
const gateway = createOpenAI({
  baseURL: 'https://llm-gateway.internal/v1',
  apiKey: process.env.GATEWAY_KEY,
  headers: { 'x-team': 'search' },
});
await generateText({ model: gateway('gpt-4.1'), prompt: 'Hi' });

// swap providers by swapping the model value — nothing else changes
await generateText({ model: anthropic('claude-sonnet-4-5'), prompt: 'Hi' });
```

`openai('gpt-4.1')` is a callable namespace: the same object exposes
`openai.chat(...)`, `openai.completion(...)`, `openai.embedding(...)`,
`openai.image(...)`. Capabilities are *methods on the provider*, not a flat
model enum.

The AI SDK also ships a **provider registry** so a single string resolves to a
configured provider instance — the TypeScript analog of LiteLLM's prefix
routing:

```ts
// TS — provider registry: a string like "anthropic:claude-sonnet-4-5" resolves
import { createProviderRegistry, generateText } from 'ai';
import { openai } from '@ai-sdk/openai';
import { anthropic } from '@ai-sdk/anthropic';

const registry = createProviderRegistry({
  openai,
  anthropic,
  // a custom-prefixed gateway:
  internal: createOpenAI({ baseURL: 'https://gw/v1', apiKey: process.env.GW }),
});

// languageModel("<providerId>:<modelId>")
await generateText({ model: registry.languageModel('anthropic:claude-sonnet-4-5'), prompt: 'Hi' });
await generateText({ model: registry.languageModel('internal:gpt-4.1'), prompt: 'Hi' });
```

Underneath both bridges sits the same idea: a tiny model interface
(`LanguageModelV2`'s `doGenerate` / `doStream`) that each provider implements by
normalizing one canonical prompt shape *to* its native wire format. Routing
decisions never consult `provider`/`modelId` once you hold a model handle — they
are pure metadata for telemetry.

The OpenAI Agents SDK takes a third route: a `ModelProvider.get_model(name) ->
Model` resolver turns the string the user wrote into a concrete `Model`. Its
default ships two implementations of OpenAI's *own* protocol
(`OpenAIResponsesModel`, `OpenAIChatCompletionsModel`) plus a `LitellmModel`
bridge for everything else.

```python
# Python — OpenAI Agents SDK: route a single agent to a non-OpenAI model via LiteLLM
from agents import Agent
from agents.extensions.models.litellm_model import LitellmModel

agent = Agent(
    name="Claude-powered",
    instructions="...",
    model=LitellmModel(model="anthropic/claude-sonnet-4-5"),
)
```

### What varies across providers

| Dimension | OpenAI-compatible proxies | First-party APIs | Universal bridges |
|---|---|---|---|
| Wire shape | Chat Completions | each native | normalized to one shape |
| Switch by | `base_url` + key | new SDK + new code | model-string prefix |
| Routing knob | endpoint URL | hard-coded SDK | prefix / registry id |
| Headers | passthrough | provider-specific | bridge maps them |
| Feature ceiling | whatever the proxy forwards | full native feature set | the bridge's lowest common denominator |

- **Capability gaps.** A bridge that normalizes to Chat Completions silently
  loses features that have no Chat Completions equivalent: hosted tools,
  reasoning items, `previous_response_id`, Anthropic thinking blocks, Gemini
  safety ratings. The OpenAI Agents SDK demonstrates this within one vendor:
  `OpenAIChatCompletionsModel` no-ops features that `OpenAIResponsesModel`
  supports.
- **Auth shape.** OpenAI uses `Authorization: Bearer`; Anthropic uses
  `x-api-key` + `anthropic-version`; Gemini puts the key in a `?key=` query
  param or a header; Bedrock/Vertex use cloud IAM (SigV4 / OAuth), not a
  bearer key at all.
- **Model id namespacing.** `gpt-4.1` vs `claude-sonnet-4-5` vs
  `gemini-2.5-flash` vs Bedrock's `anthropic.claude-3-5-sonnet-...-v2:0`. The
  prefix scheme (`anthropic/...`, `openrouter/...`, `bedrock/...`) exists
  precisely to disambiguate these.

### What's hard

- **The lowest-common-denominator trap.** A universal surface only exposes what
  *all* backends share. The moment you want a provider-specific feature
  (Anthropic cache breakpoints, Gemini safety settings, OpenAI structured
  outputs strict mode), you need an escape hatch — Vercel's `providerOptions`,
  LiteLLM's pass-through kwargs. Designing the escape hatch is as hard as
  designing the common surface.
- **Prompt normalization is real work, in both directions.** Mapping one
  canonical message array to OpenAI/Anthropic/Gemini wire format (and back) is
  non-trivial: role coalescing, system-prompt hoisting, tool-result placement,
  schema-dialect conversion (JSON Schema → Gemini's uppercase OpenAPI subset).
  Every bridge absorbs this; getting a round-trip lossless (e.g. thinking
  blocks) is the hard part.
- **"OpenAI-compatible" is a spectrum, not a guarantee.** Proxies implement
  *most* of the Chat Completions surface; streaming framing, `tool_choice`,
  `response_format`, and usage reporting are where they quietly differ.

---

## ★ Reliability

**Goal:** "I want a call to survive transient failures — retry on 429/5xx, fall
back to another model when one is down, spread load across keys, and time out
instead of hanging forever."

### How it's done today

Provider SDKs ship **built-in retries with exponential backoff** and a
configurable timeout out of the box. The OpenAI and Anthropic SDKs default to
2 retries with jittered backoff and honor `Retry-After`.

```python
# Python — SDK-level retries + timeout (OpenAI; Anthropic is identical shape)
from openai import OpenAI

client = OpenAI(max_retries=4, timeout=30.0)   # retries 429/5xx with backoff
resp = client.chat.completions.create(
    model="gpt-4.1",
    messages=[{"role": "user", "content": "Hi"}],
    timeout=10.0,                               # per-request override
)
```

```ts
// TS — SDK-level retries + timeout
import OpenAI from 'openai';
const client = new OpenAI({ maxRetries: 4, timeout: 30_000 });
await client.chat.completions.create(
  { model: 'gpt-4.1', messages: [{ role: 'user', content: 'Hi' }] },
  { maxRetries: 2, timeout: 10_000 },           // per-request override
);
```

Beyond per-SDK retries, apps build **fallback chains** and **round-robin /
load-balancing** across keys or providers. LiteLLM's `Router` is the canonical
Python implementation:

```python
# Python — LiteLLM Router: round-robin across keys, with a fallback chain
from litellm import Router

router = Router(
    model_list=[
        {"model_name": "smart", "litellm_params": {"model": "openai/gpt-4.1", "api_key": KEY_A}},
        {"model_name": "smart", "litellm_params": {"model": "openai/gpt-4.1", "api_key": KEY_B}},   # 2nd key
        {"model_name": "smart", "litellm_params": {"model": "anthropic/claude-sonnet-4-5"}},        # fallback target
    ],
    routing_strategy="least-busy",          # or "usage-based-routing", "latency-based"
    fallbacks=[{"smart": ["anthropic/claude-sonnet-4-5"]}],
    num_retries=3,
    timeout=30,
    allowed_fails=2,                         # circuit-breaker: cool a model down after N fails
    cooldown_time=60,
)
resp = router.completion(model="smart", messages=[{"role": "user", "content": "Hi"}])
```

In TypeScript the equivalents are explicit: catch, classify, retry, or fall
back to a second model value.

```ts
// TS — explicit fallback chain over the Vercel AI SDK
import { generateText, APICallError } from 'ai';
import { openai } from '@ai-sdk/openai';
import { anthropic } from '@ai-sdk/anthropic';

async function withFallback(prompt: string) {
  const chain = [openai('gpt-4.1'), anthropic('claude-sonnet-4-5')];
  let lastErr: unknown;
  for (const model of chain) {
    try {
      return await generateText({ model, prompt, abortSignal: AbortSignal.timeout(15_000) });
    } catch (err) {
      lastErr = err;
      // only fall through on transient/permanent-for-this-model errors
      if (APICallError.isInstance(err) && err.statusCode === 429) continue;
      if (APICallError.isInstance(err) && (err.statusCode ?? 0) >= 500) continue;
      throw err;                              // a 400 (bad request) won't get better elsewhere
    }
  }
  throw lastErr;
}
```

**Rate-limit handling** keys on HTTP `429` plus the `Retry-After` header (and
provider-specific `x-ratelimit-*` / `anthropic-ratelimit-*` headers that
advertise remaining quota and reset time). A correct backoff *reads*
`Retry-After` rather than guessing.

```python
# Python — respect Retry-After on a 429 instead of blind backoff
import time, openai

for attempt in range(5):
    try:
        return client.chat.completions.create(model="gpt-4.1", messages=msgs)
    except openai.RateLimitError as e:
        wait = float(e.response.headers.get("retry-after", 2 ** attempt))
        time.sleep(wait)
```

An **error taxonomy** underpins all of the above — you only retry/fall-back on
the right classes:

| Class | Examples | Strategy |
|---|---|---|
| Transient | 429 rate limit, 500/502/503, connection reset, timeout | retry w/ backoff; honor `Retry-After`; then fall back |
| Permanent | 400 bad request, 401/403 auth, 404 model not found | do **not** retry; surface to caller |
| Context-length | "maximum context length exceeded" (often a 400 subtype) | truncate / summarize history, or route to a bigger-context model |
| Content filter | `finish_reason: content_filter` (OpenAI), `stop_reason`/safety block (Anthropic/Gemini) | not a transport error — a *successful* response that produced nothing usable |

```ts
// TS — classify before deciding what to do
import { APICallError } from 'ai';
function classify(err: unknown): 'transient' | 'context' | 'permanent' {
  if (!APICallError.isInstance(err)) return 'permanent';
  if (err.statusCode === 429 || (err.statusCode ?? 0) >= 500) return 'transient';
  if (/context length|maximum.*tokens/i.test(err.message)) return 'context';
  return 'permanent';
}
```

**Proactive moderation & configurable safety.** The content-filter row above is
*reactive* — you find out a response was blocked after the fact. Two other
surfaces let you act *before* the generation, or tune how aggressive the
built-in filter is.

- **Standalone moderation pass (OpenAI).** A free `omni-moderation-latest`
  endpoint classifies text (and images) into harm categories and returns
  per-category booleans *and* numeric `category_scores`, so you can gate on a
  threshold you choose rather than the model's all-or-nothing filter. Common
  uses: screen user input before spending tokens, screen model output before
  showing it, or log scores for review.

```python
# Python — OpenAI moderation: a separate, free classification call
mod = client.moderations.create(model="omni-moderation-latest", input=user_text)
r = mod.results[0]
if r.flagged:                       # any category over OpenAI's threshold
    ...                             # block / route to review
r.category_scores.violence          # 0..1 — gate on your own threshold instead
```

- **Request-time safety settings (Gemini).** `safetySettings` attach
  per-`HarmCategory` block thresholds (`HarmBlockThreshold`) to the generate
  call itself — and the threshold can be *relaxed* (`BLOCK_ONLY_HIGH`,
  `BLOCK_NONE`) for legitimate use cases the default filter over-blocks
  (medical, legal, security content). On newer models (Gemini 2.5 Flash and
  later) several categories default to `OFF`, so tightening is also a choice.

```python
# Python — Gemini: tune the built-in filter at request time
from google.genai import types
resp = client.models.generate_content(
    model="gemini-2.5-flash",
    contents="Summarize this clinical drug-interaction report.",
    config=types.GenerateContentConfig(
        safety_settings=[
            types.SafetySetting(
                category=types.HarmCategory.HARM_CATEGORY_DANGEROUS_CONTENT,
                threshold=types.HarmBlockThreshold.BLOCK_ONLY_HIGH,  # relax for medical text
            ),
        ],
    ),
)
```

Anthropic exposes no request-time threshold knob of this kind — safety is
applied server-side and surfaced reactively (the content-filter row above),
which is itself part of what varies.

### What varies across providers

- **Where retry/backoff lives.** Built into the SDK (OpenAI, Anthropic) vs.
  bolted on by a router (LiteLLM) vs. hand-rolled (raw `fetch`, Vercel AI SDK
  has retry config on some helpers but fallback is the caller's job).
- **Rate-limit signalling.** `Retry-After` is standard, but the *advisory*
  headers differ: OpenAI's `x-ratelimit-remaining-{requests,tokens}` vs
  Anthropic's `anthropic-ratelimit-*` vs Gemini's quota model (per-project
  RPM/TPM enforced server-side, often surfaced as `429` with a
  `RESOURCE_EXHAUSTED` body).
- **Usage tiers & quota ladders.** The *ceiling* you back off against is not
  fixed — it climbs as you spend. OpenAI ramps accounts through **Tier 1–5**
  (each tier unlocks higher RPM/TPM as cumulative spend crosses thresholds);
  Anthropic has analogous usage tiers; Gemini enforces per-project RPM/TPM
  quotas. Limits are usually **per-project** (and sometimes per-key) rather than
  per-org, so the same 429 means different things depending on which scope hit
  the wall, and a quota-increase request (or moving traffic to another project)
  is the fix when retry/backoff can't help.
- **Service tiers (latency/cost-for-priority knob).** A request-level
  `service_tier` field trades latency against price on the *synchronous* path —
  the inverse of the batch tradeoff below. OpenAI exposes
  `service_tier: "flex"` (cheaper, higher and more variable latency, intended
  for non-urgent work) and `service_tier: "priority"` (faster, premium-priced);
  Anthropic exposes `service_tier: "auto"` to opt into Priority Tier capacity
  with fallback to standard. Google's analog is **Provisioned Throughput** on
  Vertex AI — a pre-purchased, guaranteed-capacity reservation rather than a
  per-request flag. These sit between standard real-time pricing and the
  ~50%-off batch path as a third point on the latency/cost curve.

```python
# Python — per-request service tier (OpenAI flex; Anthropic uses service_tier="auto")
resp = client.responses.create(
    model="gpt-4.1",
    input="Score these 10k support tickets.",
    service_tier="flex",          # cheaper, higher latency; "priority" = faster, premium
)
```
- **Content-filter shape.** OpenAI uses `finish_reason: "content_filter"`;
  Anthropic uses `stop_reason` plus separate safety; Gemini uses
  `finish_reason: "SAFETY"` *plus* `promptFeedback.blockReason` *plus*
  per-candidate `safetyRatings`. A blocked Gemini prompt can return zero
  candidates — an empty success, not an error.
- **Error class names.** `openai.RateLimitError` /
  `anthropic.RateLimitError` / Gemini `ResourceExhausted` /
  Vercel `APICallError.statusCode === 429` — same condition, four shapes.

### What's hard

- **Distinguishing "retry helps" from "retry never helps."** A 429 is worth
  retrying; a 400 from a malformed schema never is; a context-length 400 needs
  a *different* fix (shrink the prompt) before any retry. Misclassifying wastes
  money and latency, or gives up too early.
- **Fallback that preserves semantics.** Falling from GPT-4.1 to Claude is easy
  for plain text and hard for everything else: structured-output strategy,
  tool-call format, token budget, and reasoning all differ. A fallback that
  changes behavior silently is worse than a clean failure.
- **Idempotency.** Retrying a non-idempotent tool-executing agent step can
  double-run side effects. Streaming makes it worse: if a stream dies mid-way,
  "retry" means re-emitting tokens the client already saw.
- **Stateful chains break under fallback.** A `previous_response_id` or a
  provider-side cache handle is meaningless to the fallback provider.

---

## ◆ Model cascades & semantic routing

**Goal:** "I want most requests handled by a cheap model and only the hard ones
to reach an expensive one — so I capture the 10–50× tier price gap without
hurting quality on the cases that need it."

### How it's done today

The reliability section falls *back* to another model on failure; routing
chooses the model *up front* (or *escalates* on a quality signal) for cost — the
same model handles, different reason. Three shapes recur:

**(1) Cheap-first cascade with escalate-on-low-confidence.** Send to the cheap
model; if the answer looks weak — low logprob/confidence, the model says "I'm not
sure", a validator/schema-parse fails, or a downstream check rejects it —
re-issue to the strong model. Most traffic settles at tier-1 prices; only the
residual pays the premium.

```python
# Python — cheap-first cascade, escalate when the cheap answer fails a check
def answer(q):
    cheap = client.chat.completions.create(
        model="gpt-4.1-mini", messages=[{"role": "user", "content": q}],
    ).choices[0].message.content
    if is_confident(cheap):          # schema parse, validator, or self-reported certainty
        return cheap
    return client.chat.completions.create(   # only hard cases reach the premium tier
        model="gpt-4.1", messages=[{"role": "user", "content": q}],
    ).choices[0].message.content
```

**(2) Classifier / semantic routing to a tier.** A fast classifier (a tiny
model, an embedding-similarity match against labeled exemplars, or a learned
router) decides *before* generation which tier a request needs, and dispatches
accordingly — no wasted cheap call on requests known to be hard.

```ts
// TS — route to a tier by a cheap upfront classification
import { generateText } from 'ai';
import { openai } from '@ai-sdk/openai';

const tier = await classifyDifficulty(prompt);          // "easy" | "hard"
const model = tier === 'hard' ? openai('gpt-4.1') : openai('gpt-4.1-mini');
await generateText({ model, prompt });
```

**(3) Judge-gated escalation.** A separate LLM-as-judge scores the cheap model's
output; below a threshold, the request is retried on the strong model. This is
the eval scoring taxonomy (see **Evaluation**) wired into the serving path
instead of a test suite.

Dedicated routers exist: **RouteLLM** (a trained router that learns a
strong-vs-weak dispatch policy from preference data), **Semantic Router**
(embedding-similarity routing over labeled utterances), and managed services
like **NotDiamond** and **Martian** that pick a model per request from a quality
estimate. Universal bridges make the dispatch itself trivial — the routing
*decision* is the work, not the model swap.

### What varies across providers

- **Confidence signal availability.** OpenAI can return `logprobs`; Anthropic
  and Gemini expose no token-level probabilities, so a cascade on those falls
  back to self-reported certainty, a validator, or a judge.
- **Tier granularity.** OpenAI mini/nano vs full, Anthropic Haiku/Sonnet/Opus,
  Gemini Flash-Lite/Flash/Pro — each family draws the cheap/strong line at a
  different ratio, so the break-even point of a cascade is provider-specific.
- **Cross-provider routing** multiplies the normalization burden of the
  provider-diversity section: the router must speak every backend's wire shape.

### What's hard

- **LLM confidence signals are unreliable.** Logprobs measure token likelihood,
  not correctness; a model is often confidently wrong, so an escalation gate
  keyed on confidence both lets bad cheap answers through and needlessly
  escalates good ones. Calibrating the threshold is per-workload tuning.
- **The router is itself a call.** A classifier or judge adds latency and cost
  to *every* request; if it is too expensive (or too slow), it eats the savings
  it was meant to create. A cascade also pays the cheap call *and* the strong
  call on every escalated request — only worth it when the escalation rate is
  low enough.
- **Quality regressions are silent.** Routing more traffic to the cheap tier to
  save money degrades exactly the cases the router misjudged, and without an
  eval harness watching score-by-tier you find out from users, not metrics.

---

## ◆ Caching

**Goal:** "I want to stop paying full price to re-send the same large prefix
(system prompt, big document, long tool catalog) on every call."

### How it's done today

Prompt caching shows up in two structurally different shapes.

**(1) Inline annotation — Anthropic `cache_control`.** You mark a *content
block* as a cache breakpoint. The cache is **content-keyed**: the provider
hashes the prefix up to the breakpoint; an identical prefix on the next call is
a cache *read*. No handle, no separate API call. TTL is `5m` (default,
"ephemeral") or `1h` (extended). You write to the cache the first time
(`cache_creation_input_tokens`, billed at a premium) and read it cheaply after
(`cache_read_input_tokens`).

```python
# Python — Anthropic inline cache: breakpoint on a big system block
import anthropic
client = anthropic.Anthropic()

msg = client.messages.create(
    model="claude-sonnet-4-5",
    max_tokens=1024,
    system=[
        {
            "type": "text",
            "text": LARGE_STYLE_GUIDE,              # tens of KB, reused every call
            "cache_control": {"type": "ephemeral"}, # ← the breakpoint (5m default)
        },
    ],
    messages=[{"role": "user", "content": "Draft a reply."}],
)
print(msg.usage.cache_creation_input_tokens, msg.usage.cache_read_input_tokens)
```

```ts
// TS — same idea via the Vercel AI SDK providerOptions
import { generateText } from 'ai';
import { anthropic } from '@ai-sdk/anthropic';

await generateText({
  model: anthropic('claude-sonnet-4-5'),
  messages: [
    {
      role: 'system',
      content: LARGE_STYLE_GUIDE,
      providerOptions: { anthropic: { cacheControl: { type: 'ephemeral' } } },
    },
    { role: 'user', content: 'Draft a reply.' },
  ],
});
```

Breakpoints can also sit on the *last tool definition* (caching a large tool
catalog) — Anthropic caches everything up to and including the marked element.

**(2) Explicit resource — Gemini `cachedContents`.** You create a cache object
out-of-band via a separate API (`POST /cachedContents`), get back an
**addressable handle** (`cachedContents/abc123`), and reference it by name on
subsequent `generateContent` calls. It has its own lifecycle: create, get,
list, update (extend TTL), delete — and you are **billed for storage** for as
long as it lives.

```python
# Python — Gemini explicit cache resource: create, reference, manage TTL, delete
from google import genai
from google.genai import types

client = genai.Client()

# 1. create the resource (separate API call) — returns an addressable handle
cache = client.caches.create(
    model="gemini-2.5-flash",
    config=types.CreateCachedContentConfig(
        system_instruction="You are a contract analyst.",
        contents=[LARGE_CONTRACT_PDF],   # the reusable prefix
        ttl="3600s",                     # storage billed while alive
    ),
)
print(cache.name)                        # "cachedContents/abc123"

# 2. reference it by name on each generate call
resp = client.models.generate_content(
    model="gemini-2.5-flash",
    contents="What is the termination clause?",
    config=types.GenerateContentConfig(cached_content=cache.name),
)
print(resp.usage_metadata.cached_content_token_count)

# 3. manage lifecycle explicitly
client.caches.update(name=cache.name, config=types.UpdateCachedContentConfig(ttl="7200s"))
client.caches.list()
client.caches.delete(name=cache.name)
```

```ts
// TS — Gemini explicit cache via @google/genai
import { GoogleGenAI } from '@google/genai';
const ai = new GoogleGenAI({});

const cache = await ai.caches.create({
  model: 'gemini-2.5-flash',
  config: { systemInstruction: 'You are a contract analyst.', contents: [LARGE_CONTRACT], ttl: '3600s' },
});
await ai.models.generateContent({
  model: 'gemini-2.5-flash',
  contents: 'What is the termination clause?',
  config: { cachedContent: cache.name },
});
await ai.caches.delete({ name: cache.name });
```

**(3) Automatic prefix caching + opt-in routing hint — OpenAI
`prompt_cache_key`.** OpenAI caches long shared *prefixes* **automatically** —
no annotation on a block, no resource to create — and discounts the cached
input tokens, reporting them as `cached_tokens` in usage. The only lever the
developer has is an *optional* `prompt_cache_key` (on the Responses and Chat
APIs): a string set consistently across requests that share a stable prefix.
The key is combined with the prefix hash to *route* similar requests to the same
cache machine; it does **not** change the model input or the prompt itself. The
cache is still keyed on the prefix — `prompt_cache_key` only steers which
backing machine serves it, which is why the prefix must stay byte-stable to hit.

```python
# Python — OpenAI automatic prefix caching with an optional routing hint
resp = client.responses.create(
    model="gpt-4.1",
    prompt_cache_key="tenant-acme-support",   # optional: route shared prefixes together
    instructions=LARGE_SYSTEM_PROMPT,         # the stable, reused prefix
    input="Draft a reply.",
)
print(resp.usage.input_tokens_details.cached_tokens)   # > 0 means a cache hit
```

```ts
// TS — same idea via the OpenAI SDK Responses API
const resp = await client.responses.create({
  model: 'gpt-4.1',
  prompt_cache_key: 'tenant-acme-support',   // optional routing hint
  instructions: LARGE_SYSTEM_PROMPT,         // stable, reused prefix
  input: 'Draft a reply.',
});
```

Guidance: keep the key stable for genuinely shared prefixes, and pick a
*granularity* that does not overload a single `(prefix, key)` pair — beyond
~15 requests/minute per pair, traffic may overflow to additional cache machines
and the hit rate drops. Too coarse a key overloads one machine; too fine
fragments the cache so prefixes are spread thin and rarely reused.

The three modes are three points on a spectrum from "zero-config automatic" to
"fully managed object":

- **Automatic + opt-in routing hint (OpenAI `prompt_cache_key`)** — the provider
  caches shared prefixes for you; the only knob is an optional string that hints
  *where* to route, never *what* to cache.
- **Inline per-block annotation (Anthropic `cache_control`)** — content-keyed,
  with explicit ephemeral breakpoint markers you place inside the request body.
- **Explicit managed resource (Gemini `cachedContents`)** — a first-class object
  with its own create / list / update-TTL / delete lifecycle and storage bill.

The contrast is the whole story:

| Aspect | Automatic + routing hint (OpenAI `prompt_cache_key`) | Inline annotation (Anthropic `cache_control`) | Explicit resource (Gemini `cachedContents`) |
|---|---|---|---|
| **Mental model** | Nothing to do; optionally hint a routing key | Mark a content block; cache is a side effect | Create an object, then reference it |
| **Lifecycle** | Automatic — provider creates & evicts; nothing to manage | Implicit — provider evicts; you just re-send the same prefix | Explicit — create / list / update-TTL / delete |
| **Cache identity** | Automatic prefix hash + optional routing key | Content-keyed (hash of the prefix) | A handle/name (`cachedContents/abc123`) |
| **Created** | Automatically on first shared-prefix request | Implicitly on first call past a breakpoint | Explicitly via a `POST /cachedContents` call |
| **Referenced by** | Stable `prompt_cache_key` (optional) | Re-sending the identical prefix | The handle/name string |
| **Granularity** | Whole stable prefix; key chooses routing fan-out | Per-content-block breakpoints (multiple allowed) | Whole prefix: system instruction + first N contents |
| **API surface** | One optional field in the normal request body | A field inside the normal request body | A separate `cachedContents` API |
| **TTL** | Provider-managed (not caller-set) | `5m` (ephemeral) or `1h` | Caller-set (e.g. `3600s`), updatable |
| **Billing** | Discounted cached reads; no idle cost | Write premium + cheap reads; no idle cost | Cheap reads **+ storage billed while alive** |
| **Failure mode if stale** | Cache miss → full-price call, transparent | Cache miss → full-price call, transparent | Dangling/expired handle → error; orphaned handle → silent storage cost |

**Implicit caching** generalizes the OpenAI automatic mode above: some
providers (OpenAI's automatic prompt caching, Gemini's implicit caching on
newer models) detect a repeated prefix and discount it automatically with no
annotation and no resource, reporting `cached_tokens` in usage. The app does
nothing; it just watches the usage object to confirm hits (Gemini exposes no
routing hint analogous to OpenAI's `prompt_cache_key`).

```python
# Python — OpenAI implicit caching: no annotation, just observe the discount
resp = client.chat.completions.create(model="gpt-4.1", messages=msgs)
print(resp.usage.prompt_tokens_details.cached_tokens)   # > 0 means a cache hit
```

### What varies across providers

- **Who owns the lifecycle.** Anthropic: nobody — re-send the prefix, accept a
  miss. Gemini: you — create and delete the resource, and pay storage.
  Implicit: nobody, and you can't control it.
- **Where the cache lives in the request.** A `cache_control` field on a block,
  a top-level `cachedContent` string, or invisible.
- **Cache key.** Content hash (Anthropic), a named handle (Gemini), or an
  internal heuristic (implicit).
- **Usage reporting.** `cache_creation_input_tokens` /
  `cache_read_input_tokens` (Anthropic), `cachedContentTokenCount` (Gemini),
  `prompt_tokens_details.cached_tokens` (OpenAI).

### What's hard

- **Two shapes, one abstraction.** A generic "cache this" flag maps cleanly to
  Anthropic's inline model but awkwardly to Gemini's: someone has to *own the
  resource lifecycle* (create, TTL refresh, delete) and the storage bill. A
  framework either hides this (and must garbage-collect handles) or exposes it
  (and leaks Gemini-specific concepts).
- **Breakpoint placement matters.** Inline caching only helps if the cached
  prefix is *byte-stable* and at the front. A timestamp, a request id, or a
  reordered tool list at the top busts the cache for everything after it.
- **TTL economics.** A `1h` Anthropic cache or a long-lived Gemini resource is
  only worth it above a hit-rate threshold; below it you pay the write premium
  (or storage) for nothing.
- **Cache hits depend on an *exact stable prefix*.** Automatic and inline
  caching both key on a byte-stable prefix, so any prefix edit busts the cache
  for everything after it: reordering tool definitions (see the tool-ordering
  note in [`02-tools-and-agents.md`](02-tools-and-agents.md)), or rewriting
  history during compaction (see [`03-state-sessions-memory.md`](03-state-sessions-memory.md))
  silently turns every subsequent call into a full-price miss. A routing hint
  like `prompt_cache_key` cannot rescue a prefix that is no longer identical.
- **Routing-key granularity is a tradeoff.** With OpenAI's `prompt_cache_key`,
  too *coarse* a key funnels too much traffic at one `(prefix, key)` pair and
  overflows it to extra machines (past ~15 req/min) — lowering the hit rate;
  too *fine* a key fragments the cache so each prefix is rarely reused. Picking
  the granularity is per-workload tuning with no universal right answer.
- **Caching interacts with fallback.** A cache handle/prefix is provider-local;
  switching providers throws away the cache and re-pays the full prefix.

---

## ◆ Observability

**Goal:** "I want to see every model call and tool call as a trace, know its
token cost, and ship that to my observability backend."
(Scoring those traces is its own concern — see **Evaluation** below.)

### How it's done today

Two complementary layers:

**Tracing spans.** Each model call and each tool call becomes a span, nested
under the run. Some SDKs instrument automatically — the OpenAI Agents SDK emits
a `generation_span` per model call and a `function_span` per tool call without
the provider author doing anything; the Runner wraps each call. Most apps export
these to **Langfuse, Braintrust, or OpenTelemetry** collectors.

```python
# Python — OpenAI Agents SDK: spans are automatic; add a backend exporter
from agents import Runner, add_trace_processor
from langfuse.openai_agents import LangfuseTraceProcessor   # or an OTel processor

add_trace_processor(LangfuseTraceProcessor())
result = Runner.run_sync(agent, "Summarize Q3 revenue.")
# every model call + tool call now appears as a nested span in Langfuse
```

```python
# Python — OpenTelemetry as the vendor-neutral backend (via OpenInference/OTel)
from opentelemetry import trace
tracer = trace.get_tracer("llm-app")

with tracer.start_as_current_span("llm.call") as span:
    resp = client.chat.completions.create(model="gpt-4.1", messages=msgs)
    span.set_attribute("gen_ai.usage.input_tokens", resp.usage.prompt_tokens)
    span.set_attribute("gen_ai.usage.output_tokens", resp.usage.completion_tokens)
    span.set_attribute("gen_ai.response.model", resp.model)
```

In TypeScript the Vercel AI SDK has built-in OpenTelemetry support: flip
`experimental_telemetry` on and every `generateText`/`streamText` emits spans
with usage attributes that Langfuse/Braintrust collectors read.

```ts
// TS — Vercel AI SDK telemetry → OpenTelemetry → any collector
import { generateText } from 'ai';
import { openai } from '@ai-sdk/openai';

await generateText({
  model: openai('gpt-4.1'),
  prompt: 'Summarize Q3 revenue.',
  experimental_telemetry: {
    isEnabled: true,
    functionId: 'q3-summary',
    metadata: { userId: 'u_42', tenant: 'acme' },
  },
});
```

**Token & cost accounting** rides on the usage object every response carries
(see the next section). Collectors multiply usage by a per-model price table to
attribute cost per span, per user, per feature. Recorded traces also feed the
**Evaluation** section below — production traffic becomes the test set.

Some frameworks fold tracing into the framework itself. **Pydantic AI**
pairs with **Logfire** — an OpenTelemetry-based platform — so a single
`logfire.instrument_pydantic_ai()` call traces every run (its companion
**Pydantic Evals** scores those recorded spans — see **Evaluation**):

```python
# Python — Pydantic AI: one call instruments every agent run to an OTel backend
import logfire
logfire.configure()
logfire.instrument_pydantic_ai()      # all agent runs now traced (Logfire / any OTel backend)
```

Observability also extends to durable, long-running orchestration: workflow
engines emit a span (and a replayable checkpoint) per step — see
[`07-workflows-and-orchestration.md`](07-workflows-and-orchestration.md).

The **wire-message vs persisted-message distinction** is the structural insight
underneath observability. The array you send the model (the *wire* /
`ModelMessage` shape) is flat and normalized; what you *persist and render* (the
*UI* / `UIMessage` shape) carries ids, timestamps, tool-call streaming states,
reasoning parts, and metadata. The Vercel AI SDK makes this explicit:
`ModelMessage` ↔ `LanguageModelV2Prompt` is the wire format; `UIMessage` is the
persistence/render format, and conversion is one-way per direction
(`toUIMessageStreamResponse` out, `convertToModelMessages` in). Observability
records the *UIMessage* (rich, for humans) while the model only ever sees the
*ModelMessage* (lean, for the API). Conflating them is the common mistake.

### What varies across providers

- **Auto-instrumentation vs manual.** OpenAI Agents SDK and Vercel AI SDK emit
  spans for you; a raw SDK call instruments nothing — you wrap it yourself.
- **Usage field names** differ per provider (next section), so a collector
  needs a per-provider adapter to populate the same span attributes.
- **What's traceable.** Hosted-tool calls, reasoning tokens, and cache
  read/write counts are visible on rich APIs (Responses, Anthropic Messages)
  and absent on Chat-Completions-normalized paths.
- **Semantic conventions.** OpenTelemetry's `gen_ai.*` attributes are
  stabilizing but vendors still emit slightly different keys; Langfuse and
  Braintrust each have their own native schema plus an OTel bridge.

### What's hard

- **Spans across an agent loop.** One user turn can be N model calls + M tool
  calls; correctly nesting them (and attributing cost up the tree) requires the
  tracer to understand the loop, not just individual calls.
- **Streaming spans.** A span that opens on first byte and closes on the last —
  with token counts only known at the end — is awkward to model; partial
  failures mid-stream leave dangling spans.
- **Capturing the right message shape.** Logging the wire `ModelMessage` loses
  rendering context; logging the `UIMessage` can leak PII and balloon storage.
- **Sampling and PII.** Full prompt/response capture is the most useful and the
  most dangerous; redaction has to happen before export.

---

## ★ Evaluation

**Goal:** "I want to measure whether my prompts, models, and pipelines actually
work — score outputs against a dataset, catch regressions before they ship, and
keep score as I change the prompt or swap the model."

### How it's done today

Evaluation is non-deterministic-software's substitute for the unit test. The
loop has four parts: a **dataset**, a **task** (the thing under test), one or
more **scorers**, and a **runner** that pairs them and aggregates.

**Building & curating datasets.** The dataset is the asset. Three sources
combine:

- **Golden sets** — hand-curated input→expected pairs that encode the behavior
  you care about (the canonical "must always pass" cases).
- **Holdouts** — cases withheld from prompt tuning so a score on them reflects
  generalization, not overfitting to the examples you iterated against.
- **Synthetic cases** — model-generated inputs (edge cases, paraphrases,
  adversarial variants) to widen coverage cheaply, plus **production traffic**
  promoted from recorded traces (see **Observability**), which is the highest-
  signal source because it is what users actually send.

**The scoring taxonomy.** No single metric fits; apps stack several:

| Scorer | What it measures | Cost / determinism |
|---|---|---|
| Exact / fuzzy match | output equals (or ~equals) a reference string | free, deterministic |
| Code assertions | a predicate over the output (JSON parses, schema valid, contains X) | free, deterministic |
| Embedding similarity | semantic closeness to a reference answer | cheap, deterministic-ish |
| Numeric / rubric grader | a 1–5 score against a written rubric | LLM call (or human) |
| LLM-as-judge | a model decides pass/fail or scores against criteria | LLM call, non-deterministic |
| Pairwise preference | a judge picks A vs B (good for "is the new version better?") | LLM call, non-deterministic |

Cheap deterministic scorers (assertions, exact match) gate the bulk; LLM-judge
scorers handle the open-ended cases where no reference string exists.

```python
# Python — an eval: dataset + task + a mix of scorers, with a runner
from braintrust import Eval
from autoevals import Factuality, Levenshtein   # LLM-judge + deterministic scorers

Eval(
    "support-bot",
    data=lambda: load_recorded_cases(),          # golden + holdout + promoted-from-prod
    task=lambda input: run_my_pipeline(input),   # the thing under test
    scores=[Factuality(), Levenshtein()],        # LLM-as-judge + string distance
)
```

**Regression testing & CI gating.** Evals run in CI on every prompt/model
change and **fail the build on a score drop** (an absolute floor, or a delta vs
the last green run). To make that signal stable rather than noisy, eval runs
lean on the same determinism levers CI uses elsewhere — low/zero temperature, a
fixed `seed` where supported, pinned model versions — so a red build means the
*change* regressed, not that sampling drifted (the seed/temperature note in
**Deployment shapes → CI**).

```ts
// TS — gate CI on an eval score (Vercel AI SDK task + threshold check)
import { generateText } from 'ai';
import { openai } from '@ai-sdk/openai';

let pass = 0;
for (const c of goldenCases) {
  const { text } = await generateText({
    model: openai('gpt-4.1'), prompt: c.input, temperature: 0, seed: 7,  // deterministic-as-possible
  });
  if (await judge(text, c.expected)) pass++;
}
const score = pass / goldenCases.length;
if (score < 0.95) { console.error(`eval regressed: ${score}`); process.exit(1); }  // fail the build
```

**LLM-as-judge pitfalls.** A judge is itself a fallible model, with documented
biases the eval has to defend against:

- **Position bias** — in pairwise judging, the model favors whichever answer is
  shown first (or second). Mitigate by swapping order and averaging.
- **Self-preference** — a judge tends to rate outputs from its own model family
  higher; using a different family (or several judges) reduces it.
- **Calibration** — raw 1–10 scores cluster and drift; a written rubric with
  anchored examples, or reducing to pass/fail, is steadier than a bare scale.
- **Judge drift** — the judge model updates underneath you, so historical
  scores stop being comparable; pinning the judge's model version keeps the
  scale stable across runs.

**Eval-driven development** turns this into a workflow: write the eval first,
change the prompt/model, re-run, keep the change only if the score holds or
improves — the same edit→test→keep loop as TDD, with a score instead of a green
bar. **Adversarial eval / red-teaming** is the dataset's stress half: jailbreak
prompts, prompt-injection payloads, PII-extraction attempts, and unsafe-content
probes run as their own suite, scored on refusal/safety rather than correctness,
to find failures before an attacker does.

**Tools.** The space is crowded: **OpenAI Evals** (provider-native runner),
**LangSmith**, **Braintrust** (above), **DeepEval** (pytest-style assertions),
**Promptfoo** (config-driven, CI-first), **Ragas** (RAG-specific metrics —
faithfulness, context precision/recall), **Inspect** (AISI's framework, strong
on agentic and safety evals), and the framework-native runners (**Pydantic
Evals**, LangSmith's evaluators).

### What varies across providers

- **Determinism levers.** OpenAI exposes a `seed` (best-effort, with a
  `system_fingerprint` to detect backend changes); Anthropic and Gemini have no
  seed, so `temperature: 0` is the only reproducibility knob — which makes
  exact-match regression scores noisier on those backends.
- **Logprobs for scoring.** Available on OpenAI, absent on Anthropic/Gemini, so
  perplexity- or confidence-based scorers aren't portable.
- **Judge availability.** Any model can be a judge, but cost and bias differ;
  cross-family judging (to dodge self-preference) means the eval harness itself
  has to be multi-provider.

### What's hard

- **The dataset is the bottleneck, not the runner.** Wiring up a scorer is easy;
  curating enough representative, correctly-labeled cases — and keeping holdouts
  truly held out — is the sustained work, and a stale dataset silently stops
  reflecting production.
- **Grading the grader.** An LLM judge needs its *own* validation against human
  labels, or you are optimizing toward a biased proxy; without that, a rising
  eval score can mean the judge drifted, not that the product improved.
- **Score stability vs reality.** Pushing temperature to 0 and pinning seeds
  makes CI stable but tests a model in a mode users never see (they get sampled,
  varied output); a passing deterministic eval can still mask real-world
  variance.
- **Evals cost real money and time.** A full LLM-judge sweep over a large
  dataset is itself a big batch of model calls — which is exactly the workload
  the batch API below exists for.

---

## ★ Cost & tokens

**Goal:** "I want to know what each call cost, budget across a multi-call
pipeline, and not get surprised by a bill."

### How it's done today

Every response carries a **usage object**. The fields have grown well past
"input/output" to include reasoning and cached tokens:

```python
# Python — usage shapes per provider (same call, three vocabularies)

# OpenAI Chat Completions / Responses
u = resp.usage
u.prompt_tokens; u.completion_tokens; u.total_tokens
u.prompt_tokens_details.cached_tokens          # cache hits
u.completion_tokens_details.reasoning_tokens   # o-series / reasoning models

# Anthropic Messages
u = msg.usage
u.input_tokens; u.output_tokens
u.cache_creation_input_tokens; u.cache_read_input_tokens

# Gemini
u = resp.usage_metadata
u.prompt_token_count; u.candidates_token_count; u.total_token_count
u.cached_content_token_count
```

```ts
// TS — Vercel AI SDK normalizes usage across providers
const { usage, totalUsage } = await generateText({ model: openai('gpt-4.1'), prompt });
usage.inputTokens; usage.outputTokens; usage.totalTokens;
usage.reasoningTokens; usage.cachedInputTokens;   // normalized fields
// in an agent loop, totalUsage aggregates every step
```

**Cost** is usage multiplied along several **pricing axes**:

- **Direction** — input tokens vs output tokens are priced differently
  (output is typically 3–5× input).
- **Cache state** — cache *writes* cost a premium over normal input; cache
  *reads* are heavily discounted; Gemini adds a per-hour *storage* charge.
- **Reasoning tokens** — billed as output even though the user never sees them.
- **Modality** — image/audio input and image/audio output have their own rates.
- **Model tier** — a frontier model can be 10–50× a small one for the same
  tokens.
- **Processing mode** — the *synchronous* path is full price; an asynchronous
  **batch** submission is ~50% off the same tokens (see the batch subsection
  below), and `service_tier` (flex/priority) shifts the synchronous price up or
  down for latency. Same tokens, same model — the delivery mode is its own axis.

```python
# Python — a tiny cost accumulator across a multi-call pipeline
PRICES = {  # USD per 1M tokens: (input, output, cache_read)
    "gpt-4.1":           (2.00,  8.00, 0.50),
    "claude-sonnet-4-5": (3.00, 15.00, 0.30),
}

class Budget:
    def __init__(self, limit_usd): self.limit, self.spent = limit_usd, 0.0
    def charge(self, model, inp, out, cached=0):
        i, o, c = PRICES[model]
        cost = ((inp - cached) * i + cached * c + out * o) / 1_000_000
        self.spent += cost
        if self.spent > self.limit:
            raise RuntimeError(f"budget exceeded: ${self.spent:.2f} > ${self.limit:.2f}")
        return cost

budget = Budget(1.00)
for step in pipeline_steps:
    r = call_model(step)
    budget.charge(r.model, r.usage.prompt_tokens, r.usage.completion_tokens,
                  cached=r.usage.prompt_tokens_details.cached_tokens)
```

**Budgeting/accounting across a pipeline** means summing usage over every call
in a run — agent loops, sub-agents, retries, fallbacks all count — and often
attributing it to a user/tenant/feature for chargeback. Routers (LiteLLM) and
gateways enforce per-key budgets server-side; the OpenAI Agents SDK's
`totalUsage` and the Vercel SDK's `totalUsage` aggregate per-run.

#### ◆ Batch / async inference

When **no user is waiting** — dataset generation, eval sweeps (see
**Evaluation**), offline scoring/classification of a backlog, bulk
summarization — the **batch API** trades latency for roughly **half the token
price**. You hand the provider a file of requests, it processes them off-peak,
and you collect results later. This is the "favors batch APIs" path referenced
in **Deployment shapes → Cron / scheduled**: cron and scheduled jobs are the
natural home for it because nobody is watching the clock.

All three majors offer the same lifecycle with different surfaces:

| Provider | Submit | Per-batch cap | Window | Discount |
|---|---|---|---|---|
| OpenAI `/v1/batches` | upload a **JSONL** file, then create a batch over it | up to ~50,000 requests / 200 MB file | 24h | ~50% |
| Anthropic `messages.batches.create` | inline array of request objects | up to ~10,000 requests | 24h | ~50% |
| Gemini batch mode | a JSONL file (or an inline request list) | large (file-scale) | 24h target | ~50% |

The shape is **submit → poll → download**, and batch traffic draws on a
**separate rate-limit pool** from the synchronous API — so a batch job doesn't
eat the RPM/TPM your live requests need (and vice versa).

```python
# Python — OpenAI: JSONL submit → poll → download
batch_input = client.files.create(file=open("requests.jsonl", "rb"), purpose="batch")
batch = client.batches.create(
    input_file_id=batch_input.id,
    endpoint="/v1/chat/completions",
    completion_window="24h",            # the only window today; ~50% off, separate limit pool
)
# ...poll until terminal...
batch = client.batches.retrieve(batch.id)   # status: validating → in_progress → completed
if batch.status == "completed":
    out = client.files.content(batch.output_file_id)   # JSONL of results, one line per request
```

Each JSONL line carries a `custom_id` so results — which come back **unordered**
— can be rejoined to inputs.

```python
# Python — Anthropic: inline requests, each with a custom_id; poll then stream results
import anthropic
client = anthropic.Anthropic()

batch = client.messages.batches.create(requests=[
    {"custom_id": "case-1",
     "params": {"model": "claude-sonnet-4-5", "max_tokens": 1024,
                "messages": [{"role": "user", "content": "Score ticket 1."}]}},
    # ... up to ~10k requests ...
])
# poll batch.processing_status until "ended", then:
for r in client.messages.batches.results(batch.id):   # ~50% off; 24h window
    print(r.custom_id, r.result)
```

```ts
// TS — OpenAI batch lifecycle, same submit→poll→download shape
const file = await client.files.create({ file: jsonlBlob, purpose: 'batch' });
const batch = await client.batches.create({
  input_file_id: file.id,
  endpoint: '/v1/chat/completions',
  completion_window: '24h',
});
// later: const done = await client.batches.retrieve(batch.id);
//        if (done.status === 'completed') await client.files.content(done.output_file_id!);
```

The economics are simple: above a few hundred non-urgent requests, the ~50%
discount plus the separate rate-limit pool make batch the default for offline
work — the cost the synchronous `service_tier: "priority"` path sits at the
*opposite* end of.

### What varies across providers

- **Vocabulary**: `prompt_tokens` vs `input_tokens` vs `prompt_token_count`;
  `completion_tokens` vs `output_tokens` vs `candidates_token_count`.
- **Whether reasoning tokens are itemized** (OpenAI o-series breaks them out;
  others fold them into output).
- **Whether usage is reported on streaming**: some providers omit usage from
  the stream unless you opt in (OpenAI `stream_options: {include_usage: true}`);
  Anthropic reports input usage in `message_start` and output in
  `message_delta`.
- **Cache accounting**: writes vs reads itemized differently, and Gemini's
  storage cost is time-based, not token-based.

### What's hard

- **Counting before you send.** You can't always know input cost in advance —
  tokenizers differ per model, and the only exact source is the provider's
  count *after* the call. Pre-flight estimates use approximations (`tiktoken`,
  char/4) that drift, especially for images/tools.
- **Attributing cost in a loop.** A single user request can fan out into many
  billed calls (tool steps, sub-agents, retries). Mapping that tree back to one
  "this request cost $X" number requires aggregating across the whole run.
- **Hidden output.** Reasoning tokens are billed but invisible, so a "cheap"
  prompt can produce a large bill the user can't see.
- **Price drift.** Per-model prices change; a hard-coded table goes stale, and
  per-region (Bedrock/Vertex) and cache-state pricing multiply the matrix.

---

## ◆ Deployment shapes

**Goal:** "I want to run this code where my app actually lives — a server, a
browser, an edge function, CI, a durable object, a cron job — without leaking
keys or holding connections that can't survive there."

### How it's done today

Where the LLM call runs dictates how secrets and connections work:

- **Long-lived server** (container, VM, traditional backend). The default:
  API key in an env var / secret manager, full connection lifetime, streaming
  and WebSockets are fine.
- **Browser.** You must **never** ship a provider key to the client. The
  pattern is **ephemeral keys**: the browser asks your server, your server mints
  a short-lived token scoped to one session, the browser uses *that*. This is
  how OpenAI Realtime works in the browser (see `04-realtime-and-transports.md`).
- **Edge / serverless** (Cloudflare Workers, Vercel Edge, Lambda). Short
  execution budget, no persistent filesystem, cold starts. Streaming works but
  you can't hold a connection open between invocations. Keys come from the
  platform's secret binding, not a `.env` file.
- **CI.** Keys from the CI secret store; calls are usually for evals/tests;
  determinism (seed, low temperature) and cost caps matter more than latency.
- **Durable objects / stateful serverless** (Cloudflare Durable Objects). A
  single addressable instance can hold a WebSocket and conversation state — the
  one serverless shape that *can* host a long-lived realtime session.
- **Cron / scheduled.** No user waiting; favors the ~50%-off **batch API** (see
  **Cost & tokens → Batch / async inference**), generous timeouts, and
  aggressive retry/fallback because nobody is watching.

```ts
// TS — server mints an ephemeral, session-scoped token; the browser never sees the real key
// (server, e.g. Next.js route handler)
export async function POST() {
  const r = await fetch('https://api.openai.com/v1/realtime/client_secrets', {
    method: 'POST',
    headers: { Authorization: `Bearer ${process.env.OPENAI_API_KEY}`, 'Content-Type': 'application/json' },
    body: JSON.stringify({ session: { type: 'realtime', model: 'gpt-realtime' } }),
  });
  const { value } = await r.json();      // short-lived client secret
  return Response.json({ token: value }); // browser uses THIS, time-boxed to one session
}
```

```ts
// TS — edge runtime: secret from platform binding, streaming response, no fs
export const runtime = 'edge';
import { streamText } from 'ai';
import { createOpenAI } from '@ai-sdk/openai';

export async function POST(req: Request, env: { OPENAI_API_KEY: string }) {
  const openai = createOpenAI({ apiKey: env.OPENAI_API_KEY });   // platform binding, not .env
  const result = streamText({ model: openai('gpt-4.1'), prompt: await req.text() });
  return result.toTextStreamResponse();                          // streams within the edge budget
}
```

```python
# Python — server-side: key from a secret manager, generous timeout for a cron job
import os
from openai import OpenAI
client = OpenAI(api_key=os.environ["OPENAI_API_KEY"], timeout=120.0, max_retries=5)
# a scheduled job: no user waiting → big timeout, aggressive retries
```

### What varies across providers

- **Ephemeral-key support.** First-class for OpenAI Realtime; for plain HTTP
  APIs the proxy-through-your-server pattern is the only safe browser option
  (your server holds the key and forwards requests).
- **Cloud-IAM auth** (Bedrock, Vertex) fits server/edge differently than a
  bearer key: it needs the cloud SDK's credential chain, which may not exist in
  a browser or a minimal edge runtime.
- **Streaming transport viability** by environment: SSE/chunked-JSON work
  almost everywhere; WebSockets/WebRTC need a host that can hold a socket
  (server, durable object) — not a plain serverless function.

### What's hard

- **Secret hygiene under bundlers.** A key in client-bundled code is a leak;
  the build toolchain has to guarantee the key never crosses into the client
  bundle.
- **Connection lifetime vs platform limits.** A 30-second realtime session
  outlives a serverless function's execution budget; only durable/stateful
  hosts bridge the gap.
- **Cold starts vs caching.** Edge cold starts re-establish connections and
  lose any in-process state (including local prompt-cache bookkeeping).
- **The same code, four homes.** Code written for a long-lived server assumes a
  filesystem and persistent connections that edge/serverless don't have; making
  one provider layer run unchanged in all of them is a real constraint.

---

## ◆ Capability negotiation

**Goal:** "I want to know *before* I send whether a model can do what I'm asking
— image input, JSON-schema output, `tool_choice: required`, a 1M context — and
not just discover it via a 400."

### How it's done today

Mostly **at runtime**: you send the request and the provider rejects it.

```python
# Python — the dominant pattern: try it, catch the 400
try:
    resp = client.chat.completions.create(
        model="some-model",
        messages=msgs,
        response_format={"type": "json_schema", "json_schema": SCHEMA, "strict": True},
    )
except openai.BadRequestError as e:
    # "model does not support structured outputs" → fall back to prompt-based JSON
    resp = client.chat.completions.create(model="some-model", messages=add_json_instructions(msgs))
```

Some surfaces are **declarative**. The Vercel AI SDK exposes `supportedUrls` on
the model — a map of URL patterns the model can ingest natively. The SDK reads
it as *data* and decides whether to forward a URL or download the bytes itself:

```ts
// TS — capability negotiation as data: supportedUrls on the model
const model = openai('gpt-4.1');
const supported = await model.supportedUrls;
// e.g. { 'image/*': [/^https:\/\/.+/] }
// when a user attaches a URL file part, the SDK checks this map:
//   match → forward the URL;  no match → fetch the bytes and send them inline.
```

The AI SDK also carries **capability flags** as fields/metadata that
higher-level helpers consult to pick a strategy, and providers emit
**`warnings`** from `doGenerate` when they silently drop or downgrade an
unsupported option (e.g. a `seed` a model ignores) rather than failing.

Beyond per-call flags, a few sources let you *enumerate* capabilities:

```python
# Python — LiteLLM's static capability table (community-maintained metadata)
import litellm
litellm.supports_function_calling(model="gpt-4.1")        # True
litellm.supports_vision(model="claude-sonnet-4-5")        # True
litellm.get_model_info("gpt-4.1")["max_input_tokens"]     # context window
```

```ts
// TS — Claude Agent SDK can ask the running model what it supports
import { query } from '@anthropic-ai/claude-agent-sdk';
const q = query({ prompt: 'hi' });
const models = await q.supportedModels();     // ModelInfo[] from the live runtime
const cmds   = await q.supportedCommands();
```

### What varies across providers

- **Discovery mechanism.** Runtime 400 (everyone) vs declarative
  `supportedUrls`/flags (Vercel) vs a static metadata table (LiteLLM) vs a
  live query to the runtime (Claude Agent SDK).
- **Structured-output capability.** OpenAI has a `strict` toggle with a
  non-strict fallback; Anthropic has no `response_format` at all (uses a
  synthetic `_return` tool); Gemini's `responseSchema` is *always* constrained
  and *rejects* unsupported schemas with a 400 — no fallback mode.
- **Schema dialect.** JSON Schema (OpenAI/Anthropic tools) vs Gemini's
  uppercase OpenAPI subset (no `$ref`, no `oneOf`/`anyOf`, closed objects) —
  a capability mismatch that only surfaces when the converted schema is
  rejected.
- **Tool/feature flags.** Parallel tool calls, `tool_choice: required`, hosted
  tools, image/audio input, reasoning, context-window size — each provider
  supports a different subset, advertised inconsistently (or not at all).

### What's hard

- **Most mismatches surface at runtime.** There is no universal, machine-readable
  capability manifest. The honest default is "try it, get a 400, adapt" — which
  means failures appear in production rather than at build/config time.
- **Capability tables go stale.** Static metadata (LiteLLM's table, hard-coded
  flags) lags new models and silently misreports; a model gains vision support
  and the table still says no.
- **Silent downgrades are worse than errors.** A provider that *ignores* an
  unsupported option (a `seed` that does nothing, a safety setting that's
  dropped) produces wrong-but-successful results. `warnings` arrays exist to
  catch this, but only if the caller reads them.
- **"Supported" is not binary.** A model may accept JSON-schema output but only
  for shallow schemas; accept images but not at a given resolution; accept tools
  but not in parallel. A flag can't capture the gradient, so the runtime probe
  remains the source of truth.

---

## What varies / what's hard

A condensed map of the divergence each concern forces a framework to absorb:

| Concern | What varies across providers | What's hard (the part a framework absorbs) |
|---|---|---|
| **Provider diversity & gateways** | Wire shape, auth, model-id namespacing; OpenAI-compatible ≠ identical | Lowest-common-denominator surface + per-provider escape hatch; lossless prompt normalization both ways |
| **Reliability** | Where retry lives; rate-limit headers; usage/service tiers; content-filter & moderation shape; error class names | Classifying retry-helps vs never-helps; semantics-preserving fallback; idempotency under retry/streaming |
| **Model cascades & routing** | Confidence-signal availability (logprobs); tier granularity & break-even; cross-provider wire shapes | Unreliable LLM confidence; the router is itself a billed call; silent quality regressions on the cheap tier |
| **Caching** | Inline annotation vs explicit resource vs implicit; usage field names | One abstraction over two lifecycles; who owns resource cleanup + storage cost; byte-stable prefixes |
| **Observability** | Auto-instrumented vs manual; usage vocab; OTel vs native schemas | Spans across loops + streams; wire-vs-persisted message capture; PII/redaction before export |
| **Evaluation** | Determinism levers (seed vs temperature-only); logprobs; judge availability | Dataset curation is the bottleneck; grading the grader; score stability vs sampled reality; eval cost |
| **Cost & tokens** | Usage vocabulary; reasoning itemization; streaming usage opt-in; cache accounting; batch & service-tier pricing | Pre-flight counting; cost attribution across a fan-out tree; hidden reasoning tokens; price drift |
| **Deployment shapes** | Ephemeral keys vs proxy-through-server; cloud-IAM auth; transport viability | Secret hygiene under bundlers; connection lifetime vs platform limits; one code path, many hosts |
| **Capability negotiation** | Runtime 400 vs declarative flags vs static table vs live query; schema dialects | No universal manifest — most mismatches surface at runtime; stale tables; silent downgrades; non-binary support |

The throughline: each concern is the *system around the call*, and every
provider expresses it in a different vocabulary with a different lifecycle.
Whatever sits between an application and the models has to flatten that
divergence into one surface while preserving an escape hatch for the parts that
genuinely differ — and has to decide, for each concern, whether the truth is
declared up front or discovered at runtime.
