# 01 — Getting one answer from a model

> The single-turn surface: you send a prompt, you get a response. No tool loops,
> no sessions, no multi-agent orchestration — just one request/response exchange
> with a language model. This is the foundation everything else builds on.

This file maps what people do when they want **one answer**: plain text, typed
JSON, a stream of tokens, an image, an audio clip, a transcript. It covers the
three dominant HTTP APIs in production today — **OpenAI Chat Completions**,
**Anthropic Messages**, and **Google Gemini `generateContent`** — and notes
OpenAI's newer **Responses** API where it diverges. Tools and the agent loop are
in `02-tools-and-agents.md`; state across turns is in `03-state-sessions-memory.md`;
streaming transports as a category are in `04-realtime-and-transports.md`.

Legend: ★ table-stakes · ◆ advanced · ▲ frontier.

---

## ★ Plain text in / text out

**Goal:** "I want to send some text and get text back."

### How it's done today

Every chat API models a conversation as an **array of messages**, each with a
`role` (`system`, `user`, `assistant`) and `content`. You send the array; the
model appends one `assistant` message. The system message steers behavior; the
user message carries the request; prior assistant messages (if any) provide
context.

```python
# Python — OpenAI Chat Completions
from openai import OpenAI
client = OpenAI()

resp = client.chat.completions.create(
    model="gpt-4.1",
    messages=[
        {"role": "system", "content": "You are a terse assistant."},
        {"role": "user", "content": "Summarize the plot of Hamlet in one line."},
    ],
)
print(resp.choices[0].message.content)
```

```python
# Python — Anthropic Messages
import anthropic
client = anthropic.Anthropic()

msg = client.messages.create(
    model="claude-sonnet-4-5",
    max_tokens=1024,                      # REQUIRED on Anthropic
    system="You are a terse assistant.",  # top-level, NOT a message
    messages=[
        {"role": "user", "content": "Summarize the plot of Hamlet in one line."},
    ],
)
print(msg.content[0].text)                # content is a list of typed blocks
```

```python
# Python — Google Gemini
from google import genai
client = genai.Client()

resp = client.models.generate_content(
    model="gemini-2.5-flash",
    contents="Summarize the plot of Hamlet in one line.",
    config={"system_instruction": "You are a terse assistant."},
)
print(resp.text)
```

```ts
// TS — OpenAI Chat Completions
import OpenAI from "openai";
const client = new OpenAI();

const resp = await client.chat.completions.create({
  model: "gpt-4.1",
  messages: [
    { role: "system", content: "You are a terse assistant." },
    { role: "user", content: "Summarize the plot of Hamlet in one line." },
  ],
});
console.log(resp.choices[0].message.content);
```

```ts
// TS — Anthropic Messages
import Anthropic from "@anthropic-ai/sdk";
const client = new Anthropic();

const msg = await client.messages.create({
  model: "claude-sonnet-4-5",
  max_tokens: 1024,
  system: "You are a terse assistant.",
  messages: [{ role: "user", content: "Summarize the plot of Hamlet in one line." }],
});
console.log(msg.content[0].type === "text" ? msg.content[0].text : "");
```

```ts
// TS — Google Gemini
import { GoogleGenAI } from "@google/genai";
const ai = new GoogleGenAI({});

const resp = await ai.models.generateContent({
  model: "gemini-2.5-flash",
  contents: "Summarize the plot of Hamlet in one line.",
  config: { systemInstruction: "You are a terse assistant." },
});
console.log(resp.text);
```

### What varies across providers

| Concern | OpenAI Chat | Anthropic | Gemini |
|---|---|---|---|
| Message container | `messages[]` | `messages[]` | `contents[]` |
| Roles | `system`/`user`/`assistant`/`tool` | `user`/`assistant` only | `user`/`model`/`function` |
| System prompt | a message with `role:"system"` | top-level `system` field | top-level `system_instruction` |
| Assistant role name | `assistant` | `assistant` | `model` |
| Content shape | string OR array of parts | array of typed blocks | array of `parts` |
| `max_tokens` | optional | **required** | optional (`maxOutputTokens`) |
| Response text path | `choices[0].message.content` | `content[0].text` | `candidates[0].content.parts[0].text` (SDK exposes `.text`) |
| Role alternation | flexible | first message must be `user`; roles should alternate | flexible |

OpenAI's **Responses** API restructures this. `input` is either a bare string or a
**list of typed _items_** — a union of `message`, `function_call`,
`function_call_output`, and `reasoning` items, not messages alone. A `message` item
is still role-keyed and still nests a `content` array of typed parts (`input_text`,
`input_image`, …), so the shape is *two-level, not flat*. The system prompt is
hoisted to a top-level `instructions` string (in-list system guidance uses the
`developer` role). The real shift is that the response's `output` items can be
passed straight back as the next request's `input` — the same item shapes flow in
both directions — which is what powers server-stored chaining and reasoning
round-tripping (see [`03-state-sessions-memory.md`](03-state-sessions-memory.md)).

### What's hard

- **There is no shared message type.** A normalized message must be re-shaped into
  three (or four) different wire formats: where does the system prompt go, what is
  the assistant role called, is content a string or a block array.
- **Extracting the answer is provider-specific.** A response can be a string, the
  first text block of a list, or the concatenation of several text parts — there's
  no single field that means "the model's text."
- **A refusal is not the answer.** When the model declines, the text you want may be
  empty and the decline lands in a *separate* field, not in the content: OpenAI exposes
  `choices[0].message.refusal` (Chat) / a `refusal` content part (Responses), and
  Anthropic returns `stop_reason: "refusal"` (with no extra message; on newer Claude
  models a `stop_details` names the policy category). Code that blindly reads "the
  text" surfaces an empty string or misclassifies a refusal as a malformed answer —
  see the structured-output section for why this matters under strict JSON.
- **Role rules differ.** Anthropic rejects a leading `assistant` message and wants
  strict `user`/`assistant` alternation; replaying a saved transcript may require
  coalescing same-role messages or inserting a placeholder `user` turn.

---

## ★ Sampling / decoding parameters

**Goal:** "I want to control *how* the model samples — how random, how repetitive,
where it stops, how many candidates."

### How it's done today

Every API exposes a set of decoding knobs alongside the prompt. The big ones —
`temperature` and `top_p` — are nearly universal; the rest (`top_k`, the two
penalties, `stop` sequences, multiple candidates, `seed`) are present on some
providers and absent on others.

```python
# Python — OpenAI Chat Completions
resp = client.chat.completions.create(
    model="gpt-4.1",
    messages=[{"role": "user", "content": "Name three colors."}],
    temperature=0.7,          # 0–2 on OpenAI
    top_p=0.9,                # nucleus sampling (use one of temperature/top_p)
    frequency_penalty=0.3,    # -2.0 … 2.0, penalize by running count
    presence_penalty=0.0,     # -2.0 … 2.0, penalize once-seen tokens
    stop=["\n\n"],            # up to 4 stop strings
    n=2,                      # number of candidate completions
    seed=42,                  # best-effort determinism
)
for choice in resp.choices:   # n>1 → multiple choices
    print(choice.message.content)
```

```python
# Python — Anthropic Messages
msg = client.messages.create(
    model="claude-sonnet-4-5",
    max_tokens=256,
    temperature=0.7,          # 0–1 on Anthropic (NOT 0–2)
    top_p=0.9,
    top_k=40,                 # Anthropic exposes top_k; OpenAI Chat does not
    stop_sequences=["\n\n"],  # note: stop_sequences, not stop
    messages=[{"role": "user", "content": "Name three colors."}],
)
# no n / candidateCount: one completion per request
```

```python
# Python — Google Gemini
resp = client.models.generate_content(
    model="gemini-2.5-flash",
    contents="Name three colors.",
    config={
        "temperature": 0.7,        # 0–2 on Gemini
        "top_p": 0.9,
        "top_k": 40,               # Gemini exposes top_k
        "stop_sequences": ["\n\n"],
        "candidate_count": 2,      # Gemini's "n"; field is candidate_count
        # frequency_penalty / presence_penalty supported on some models
    },
)
```

```ts
// TS — OpenAI Chat Completions
await client.chat.completions.create({
  model: "gpt-4.1",
  messages: [{ role: "user", content: "Name three colors." }],
  temperature: 0.7,
  top_p: 0.9,
  frequency_penalty: 0.3,
  presence_penalty: 0.0,
  stop: ["\n\n"],
  n: 2,
  seed: 42,
});
```

```ts
// TS — Anthropic Messages
await client.messages.create({
  model: "claude-sonnet-4-5",
  max_tokens: 256,
  temperature: 0.7,            // 0–1
  top_p: 0.9,
  top_k: 40,
  stop_sequences: ["\n\n"],
  messages: [{ role: "user", content: "Name three colors." }],
});
```

### What varies across providers

| Parameter | OpenAI Chat | Anthropic | Gemini |
|---|---|---|---|
| `temperature` range | 0–2 (default 1) | 0–1 (default 1) | 0–2 |
| `top_p` | yes | yes | yes |
| `top_k` | **no** (Chat Completions) | yes | yes |
| `frequency_penalty` | yes | **no** | some models |
| `presence_penalty` | yes | **no** | some models |
| Stop sequences | `stop` (≤4 strings) | `stop_sequences` | `stop_sequences` |
| Multiple candidates | `n` | **no** (one per call) | `candidate_count` |
| `seed` / determinism | yes (best-effort) + `system_fingerprint` | **no** | **no** |

`seed` (best-effort reproducibility) is essentially an OpenAI + open-weight-server
feature; OpenAI also returns a `system_fingerprint` so you can detect when the
backend changed under you. Anthropic and Gemini expose no seed. The two penalties
are an OpenAI concept; Anthropic has neither, and Gemini gates them by model.

### What's hard

- **Determinism is best-effort even with `seed`.** The same seed, prompt, and
  parameters can still diverge across model versions, hardware, batching, and
  `system_fingerprint` changes — `seed` reduces variance, it does not eliminate it,
  and most providers don't offer it at all.
- **Penalty semantics differ.** `frequency_penalty` (scaled by running count) and
  `presence_penalty` (one-shot) exist only on OpenAI (and unevenly on Gemini), so a
  prompt tuned with penalties has no equivalent knob on Anthropic — repetition control
  must move into the prompt or into `top_k`/`temperature`.
- **Provider defaults differ.** A "neutral" temperature isn't portable: 1.0 means
  different things at different ranges (a 1.0 on Anthropic's 0–1 scale is the ceiling;
  on OpenAI's 0–2 it's the midpoint), so reusing a number across providers changes how
  random the output is.
- **Single-knob abstractions leak.** Exposing one `temperature` field hides that
  `top_k`, the penalties, and `n`/`candidate_count` simply don't exist on some
  providers, so a request that sets them must drop or emulate them per backend.

---

## ◆ Logprobs / token probabilities

**Goal:** "I want the model's confidence — the log-probabilities of the tokens it
chose (and the alternatives), so I can score, calibrate, or threshold."

### How it's done today

Some APIs can attach, per output token, the log-probability the model assigned it
plus the top-N alternatives it considered. This turns a generation into a *scored*
output: classification confidence, calibration curves, and constrained-choice
scoring (compare the logprob of `"yes"` vs `"no"` instead of trusting the sampled
word).

```python
# Python — OpenAI Chat Completions: logprobs + top_logprobs
resp = client.chat.completions.create(
    model="gpt-4.1",
    messages=[{"role": "user", "content": "Is this review positive? Answer yes or no."}],
    logprobs=True,
    top_logprobs=5,            # top-5 alternatives per position (0–20)
    max_tokens=1,
)
tok = resp.choices[0].logprobs.content[0]
print(tok.token, tok.logprob)                 # chosen token + its logprob
for alt in tok.top_logprobs:                   # alternatives the model weighed
    print(alt.token, alt.logprob)
```

```python
# Python — Gemini: response_logprobs + logprobs (top-N)
resp = client.models.generate_content(
    model="gemini-2.5-flash",
    contents="Is this review positive? Answer yes or no.",
    config={"response_logprobs": True, "logprobs": 5},
)
cand = resp.candidates[0]
print(cand.avg_logprobs)                        # length-normalized confidence
print(cand.logprobs_result)                     # per-token chosen + top candidates
```

```python
# Python — OpenAI logit_bias: push the decoder toward / away from token ids
resp = client.chat.completions.create(
    model="gpt-4.1",
    messages=[{"role": "user", "content": "Pick a color."}],
    logit_bias={1131: -100},   # token id → bias (-100 = effectively banned)
)
```

```ts
// TS — OpenAI Chat Completions: logprobs
const resp = await client.chat.completions.create({
  model: "gpt-4.1",
  messages: [{ role: "user", content: "Is this review positive? Answer yes or no." }],
  logprobs: true,
  top_logprobs: 5,
  max_tokens: 1,
});
const tok = resp.choices[0].logprobs?.content?.[0];
console.log(tok?.token, tok?.logprob);
```

### What varies across providers

| Concern | OpenAI | Anthropic | Gemini |
|---|---|---|---|
| Per-token logprobs | `logprobs: true` + `top_logprobs` (Chat + Responses) | **not exposed** | `response_logprobs` + `logprobs` |
| Aggregate score | — | — | `avgLogprobs` (length-normalized) |
| Result path | `choices[0].logprobs.content[]` | n/a | `candidates[0].logprobsResult` / `avgLogprobs` |
| `logit_bias` | yes (token id → bias) | **no** | **no** |

The sharp divergence: **Anthropic exposes neither logprobs nor `logit_bias`.** There
is no way to read token probabilities or to bias the decoder by token id on the
Messages API — confidence on Anthropic has to be inferred indirectly (e.g. ask the
model to self-report, or sample multiple times), not read off the distribution.

### What's hard

- **The capability is binary and provider-bound.** Any feature built on logprobs
  (confidence thresholds, calibration, choice-scoring) simply has no Anthropic path,
  so a cross-provider design needs a fallback, not a config flag.
- **Token-level scores are tokenizer-bound.** `"yes"` may be one token or several, and
  the chosen tokenization differs per model — scoring a *word* means summing/combining
  subword logprobs correctly, not reading one number.
- **`logit_bias` is by token id**, so banning or boosting a *word* requires encoding it
  with that model's tokenizer first, and the same ids don't transfer across models.

---

## ★ Structured / typed output

**Goal:** "I want the answer as a specific JSON shape I can deserialize, not prose."

### How it's done today

This is where one desired type maps to **four different wire formats**. Everyone
wants the same thing — "give me an object matching this schema" — but each provider
exposes it differently.

**OpenAI Chat / Responses — native `json_schema` with `strict: true`.** The decoder
is constrained to the schema; the output is guaranteed to parse and validate.

```python
# Python — OpenAI Chat Completions, strict structured output
schema = {
    "type": "object",
    "properties": {
        "title":    {"type": "string"},
        "date":     {"type": "string"},
        "attendees": {"type": "array", "items": {"type": "string"}},
    },
    "required": ["title", "date", "attendees"],
    "additionalProperties": False,         # strict mode REQUIRES this
}

resp = client.chat.completions.create(
    model="gpt-4.1",
    messages=[{"role": "user", "content": "Alice and Bob meet Friday for lunch."}],
    response_format={
        "type": "json_schema",
        "json_schema": {"name": "Event", "schema": schema, "strict": True},
    },
)
event = json.loads(resp.choices[0].message.content)
```

On the **Responses** API the same schema lives under `text.format` instead of
`response_format` — same payload, different nesting (room for `audio.format` etc.).

**Anthropic — no `response_format`. The workaround is tool injection.** A synthetic
tool is declared whose `input_schema` *is* the desired type, and the model is forced
to call it; the tool input is the structured answer.

```python
# Python — Anthropic structured output via a forced "tool"
return_schema = {
    "type": "object",
    "properties": {
        "title": {"type": "string"},
        "date":  {"type": "string"},
        "attendees": {"type": "array", "items": {"type": "string"}},
    },
    "required": ["title", "date", "attendees"],
}

msg = client.messages.create(
    model="claude-sonnet-4-5",
    max_tokens=1024,
    tools=[{
        "name": "record_event",
        "description": "Return the extracted calendar event.",
        "input_schema": return_schema,
    }],
    tool_choice={"type": "tool", "name": "record_event"},   # force the call
    messages=[{"role": "user", "content": "Alice and Bob meet Friday for lunch."}],
)
event = next(b.input for b in msg.content if b.type == "tool_use")
```

**Gemini — `responseSchema` + `responseMimeType` on `generationConfig`.** Always
constrained when set; there is no strict/non-strict toggle. The schema is Gemini's
own dialect (an OpenAPI-3.0 subset with `UPPERCASE` type names, no `$ref`, no
`additionalProperties`), not standard JSON Schema.

```python
# Python — Gemini structured output
resp = client.models.generate_content(
    model="gemini-2.5-flash",
    contents="Alice and Bob meet Friday for lunch.",
    config={
        "response_mime_type": "application/json",
        "response_schema": {
            "type": "OBJECT",
            "properties": {
                "title":     {"type": "STRING"},
                "date":      {"type": "STRING"},
                "attendees": {"type": "ARRAY", "items": {"type": "STRING"}},
            },
            "required": ["title", "date", "attendees"],
        },
    },
)
event = json.loads(resp.text)
```

```ts
// TS — OpenAI Chat Completions, strict json_schema
const resp = await client.chat.completions.create({
  model: "gpt-4.1",
  messages: [{ role: "user", content: "Alice and Bob meet Friday for lunch." }],
  response_format: {
    type: "json_schema",
    json_schema: {
      name: "Event",
      strict: true,
      schema: {
        type: "object",
        properties: {
          title: { type: "string" },
          date: { type: "string" },
          attendees: { type: "array", items: { type: "string" } },
        },
        required: ["title", "date", "attendees"],
        additionalProperties: false,
      },
    },
  },
});
const event = JSON.parse(resp.choices[0].message.content ?? "{}");
```

```ts
// TS — Anthropic structured output via forced tool
const msg = await client.messages.create({
  model: "claude-sonnet-4-5",
  max_tokens: 1024,
  tools: [{ name: "record_event", description: "Return the event.", input_schema: returnSchema }],
  tool_choice: { type: "tool", name: "record_event" },
  messages: [{ role: "user", content: "Alice and Bob meet Friday for lunch." }],
});
const block = msg.content.find((b) => b.type === "tool_use");
const event = block?.input;
```

**Weaker modes.** Below schema-constrained output sit two looser options used
everywhere as fallbacks:

- **`json_object` mode** (OpenAI: `response_format: {type: "json_object"}`): the
  model is told to emit *some* valid JSON, but its shape is unconstrained. The prompt
  must describe the fields.
- **No-schema / "just ask"**: the prompt says "respond as JSON" and the caller parses
  whatever comes back. Common with models or providers that support neither
  constrained decoding nor tool injection.

**JSON coercion / repair.** Because none of the loose modes guarantee validity — and
even constrained modes can emit near-JSON when a schema is rejected or truncated by
`max_tokens` — many stacks run a repair pass: strip Markdown code fences, close
unterminated strings/brackets, coerce `"3"` to `3`, fill defaults, and re-validate.
This is the universal safety net layered under whatever wire format was used.

**Check for a refusal *before* you parse or repair.** A refusal is not malformed
JSON — it is the model declining, and it arrives in a different place than the
structured answer. Under strict JSON, OpenAI does **not** emit invalid JSON when it
refuses: the content comes back empty and the decline lands in
`choices[0].message.refusal` (Chat) or a `refusal` content part (Responses).
Anthropic signals it with `stop_reason: "refusal"` and no parseable tool input.
Running the repair pass on a refusal is exactly the wrong move — there is no JSON to
recover, and "fixing" it fabricates an object the model never produced. The correct
order is: check the refusal field / stop reason first, and only parse-or-repair when
the call actually attempted the structured answer.

### What a strict schema rejects

OpenAI strict mode (and Gemini's constrained mode) supports only a subset of JSON
Schema. Features that get a `400` (OpenAI) or are dropped/inlined (Gemini):

- recursive types via `$ref` cycles (`OrgNode { reports: OrgNode[] }`)
- `oneOf` / `anyOf` unions in many positions
- `patternProperties`, `additionalProperties: true`
- `if` / `then` / `else`
- string `pattern` / `format` constraints (varies)

The standard response is a fallback chain: try strict → if rejected, retry with
`strict: false` (or drop the unsupported keyword) → always run coercion/repair on
the result.

### What varies across providers

| | OpenAI Chat | OpenAI Responses | Anthropic | Gemini |
|---|---|---|---|---|
| Mechanism | `response_format` | `text.format` | forced tool injection | `generationConfig.responseSchema` |
| Schema dialect | JSON Schema | JSON Schema | JSON Schema (as tool input) | OpenAPI subset, UPPERCASE types |
| Guarantee toggle | `strict: true/false` | `strict: true/false` | implicit (forced call) | always constrained |
| Failure on bad schema | 400 in strict, retry non-strict | same | n/a (it's just a tool) | 400, no fallback mode |
| `json_object` mode | yes | yes | no (use tool) | `responseMimeType` only |

### What's hard

- **One type, four encodings.** The same return type compiles to `response_format`,
  `text.format`, a `_return`-style tool, or `responseSchema` — and to a *different
  schema dialect* for Gemini.
- **Schema-feature negotiation.** Deciding strict vs non-strict per type (and
  detecting cycles/unions/patterns that strict rejects) has to happen before the call.
- **Anthropic's injection has side effects.** The synthetic tool competes with real
  tools; forcing it prevents the model from calling actual tools, so when you want
  *both* structured output and tools you cannot force the return tool — you let the
  model choose, and terminate when it picks the return tool.
- **Coercion is mandatory.** Even "guaranteed" modes break under truncation or
  rejected schemas, so a repair/parse layer is needed regardless of provider.

---

## ◆ Constrained decoding beyond JSON Schema

**Goal:** "I want a guaranteed *shape* that isn't JSON — one of N choices, a regex
match, a value from a grammar — not just a JSON object."

### How it's done today

Provider strict-JSON masks the next-token distribution against a *JSON Schema*. When
you run an **open-weight model yourself** (vLLM, llama.cpp, TGI, or libraries like
Outlines and Guidance), you get the more general primitive underneath: **token-level
constrained decoding** that can enforce a regex, a context-free grammar, or a fixed
choice list — shapes a JSON schema can't express. The mask is applied at every
decode step, so the output is *guaranteed* to match by construction, not validated
after the fact.

```python
# Python — vLLM OpenAI-compatible server: guided_* extra body
from openai import OpenAI
client = OpenAI(base_url="http://localhost:8000/v1", api_key="x")

# exactly one of a fixed set
resp = client.chat.completions.create(
    model="meta-llama/Llama-3.1-8B-Instruct",
    messages=[{"role": "user", "content": "Positive or negative?"}],
    extra_body={"guided_choice": ["positive", "negative", "neutral"]},
)

# match a regex (e.g. an email, a date, a phone number)
resp = client.chat.completions.create(
    model="meta-llama/Llama-3.1-8B-Instruct",
    messages=[{"role": "user", "content": "Give me a US phone number."}],
    extra_body={"guided_regex": r"\(\d{3}\) \d{3}-\d{4}"},
)

# a context-free grammar (guided_grammar / GBNF on llama.cpp)
sql_grammar = r'''
root ::= "SELECT " columns " FROM " table
columns ::= "*" | "id" | "name"
table   ::= "users" | "orders"
'''
resp = client.chat.completions.create(
    model="meta-llama/Llama-3.1-8B-Instruct",
    messages=[{"role": "user", "content": "Query all users."}],
    extra_body={"guided_grammar": sql_grammar},
)
```

```python
# Python — Outlines: regex / choice / CFG over a local model
import outlines
model = outlines.models.transformers("meta-llama/Llama-3.1-8B-Instruct")

choice = outlines.generate.choice(model, ["positive", "negative", "neutral"])
label  = choice("Positive or negative? 'I loved it.'")

phone  = outlines.generate.regex(model, r"\(\d{3}\) \d{3}-\d{4}")
number = phone("A US phone number, please.")
```

llama.cpp expresses the same idea as **GBNF grammar files** (`--grammar`/
`grammar:` over its server), and vLLM dispatches `guided_json` / `guided_regex` /
`guided_choice` / `guided_grammar` to backends like XGrammar or the Guidance engine.

### What varies vs provider strict-JSON

- **What can be guaranteed.** Provider strict mode guarantees *JSON that validates a
  schema*. Token-level constraints guarantee *any* regex/CFG/choice — including
  non-JSON outputs (a bare enum value, a SQL string, a DSL) that a schema can't shape.
- **Where it runs.** `guided_*` / Outlines / GBNF need a model you control (or a
  serving stack that exposes them); the hosted OpenAI/Anthropic/Gemini APIs do **not**
  expose regex/CFG/choice constraints — only schema-shaped JSON (and OpenAI's strict
  mode notably drops string `pattern` anyway).
- **Cost of the constraint.** Compiling a grammar/regex into a token mask has setup
  cost (amortized when grammars are reused) and can interact with tokenization edge
  cases; provider strict-JSON hides all of that behind the API.

### What's hard

- **It's a self-hosting feature.** The strongest shape guarantees live where you run
  the weights, so a portable stack ends up with two tiers: schema-JSON on hosted
  providers, full constrained decoding on open-weight backends.
- **Grammar authoring.** Regex and CFG/GBNF are powerful but easy to get subtly wrong;
  an over-tight grammar can make a prompt unsatisfiable and stall generation.

---

## ★ Streaming tokens

**Goal:** "I want to show the answer as it's generated, not wait for the whole thing."

### How it's done today

OpenAI and Anthropic stream over **Server-Sent Events (SSE)**: the HTTP response
is a sequence of `data:` lines, each a JSON chunk. You accumulate the **text deltas**
into a buffer and watch for a **finish/stop event** that carries the stop reason and
(usually) final usage. (Gemini does *not* use SSE — see "What varies".)

```python
# Python — OpenAI Chat Completions streaming
stream = client.chat.completions.create(
    model="gpt-4.1",
    messages=[{"role": "user", "content": "Write a haiku about latency."}],
    stream=True,
    stream_options={"include_usage": True},   # usage on the final chunk
)
text = ""
for chunk in stream:
    delta = chunk.choices[0].delta.content if chunk.choices else None
    if delta:
        text += delta
        print(delta, end="", flush=True)
    if chunk.choices and chunk.choices[0].finish_reason:
        finish = chunk.choices[0].finish_reason   # "stop" | "length" | "tool_calls"
```

```python
# Python — Anthropic streaming (event grammar)
with client.messages.stream(
    model="claude-sonnet-4-5",
    max_tokens=1024,
    messages=[{"role": "user", "content": "Write a haiku about latency."}],
) as stream:
    for text in stream.text_stream:        # convenience: text deltas only
        print(text, end="", flush=True)
    final = stream.get_final_message()     # full message + usage + stop_reason
```

```ts
// TS — OpenAI Chat Completions streaming
const stream = await client.chat.completions.create({
  model: "gpt-4.1",
  messages: [{ role: "user", content: "Write a haiku about latency." }],
  stream: true,
});
let text = "";
for await (const chunk of stream) {
  const delta = chunk.choices[0]?.delta?.content;
  if (delta) { text += delta; process.stdout.write(delta); }
}
```

```ts
// TS — Anthropic streaming
const stream = client.messages.stream({
  model: "claude-sonnet-4-5",
  max_tokens: 1024,
  messages: [{ role: "user", content: "Write a haiku about latency." }],
});
stream.on("text", (delta) => process.stdout.write(delta));
const final = await stream.finalMessage();
```

Anthropic's raw SSE grammar is more structured than OpenAI's flat deltas:
`message_start` (carries input-token usage) → `content_block_start` →
`content_block_delta` (with `text_delta`, `thinking_delta`, or `input_json_delta`)
→ `content_block_stop` → `message_delta` (stop reason + output-token usage) →
`message_stop`. Each content block is tracked by index so multiple in-flight blocks
(text + thinking + tool-input) don't get interleaved.

### What varies across providers

- **Transport.** OpenAI and Anthropic use SSE. **Gemini uses chunked JSON**:
  `streamGenerateContent` returns a single HTTP response whose body is a JSON *array*
  of full `GenerateContentResponse` objects delivered incrementally — no `event:`/
  `data:` framing. An SSE parser cannot read it; you parse JSON array elements as
  they arrive.
- **Delta vs full.** OpenAI/Anthropic send *incremental* deltas. Each Gemini chunk
  is a *complete* response object whose `parts[0].text` is that chunk's increment.
- **Finish signal.** OpenAI: `finish_reason` on a choice + literal `data: [DONE]`.
  Anthropic: `message_delta.stop_reason` then `message_stop`. Gemini: `finishReason`
  on the final candidate.
- **Usage timing.** OpenAI needs `stream_options.include_usage` to get usage at all;
  it lands on the last chunk. Anthropic splits it (input tokens at start, output at
  end). Gemini reports `usageMetadata` on the final chunk(s).

### What's hard

- **Two completely different wire formats** (SSE vs chunked JSON) need two stream
  readers; "open an SSE stream" isn't a universal primitive.
- **Reassembly is stateful.** Tool-call arguments stream as fragments and must be
  concatenated by index before they parse; text from multiple blocks must be kept
  separate; thinking deltas must not be mixed into the answer.
- **Cancellation mid-stream** has to abort the HTTP request and stop accumulating
  cleanly — and you may have a partial answer to surface or discard.
- **The "final object" still has to be built** even when streaming, because tool
  loops and history want a normal message, not a pile of deltas.

---

## ◆ Streaming partial structured output

**Goal:** "I'm generating a JSON object — stream it to the UI as fields fill in,
not just at the end."

### How it's done today

When the output is structured, the token stream is JSON text arriving character by
character: `{"title": "Lun` … `ch with` … `Alice"`. A UI that waits for `}` feels
laggy. The technique is **incremental / partial JSON parsing**: feed the growing
buffer to a tolerant parser that returns the best-effort object so far, treating
unterminated strings and missing closers as "still streaming."

The Vercel AI SDK exposes this directly via `streamObject` with a `partialObjectStream`:

```ts
// TS — Vercel AI SDK, streaming a partial object
import { streamObject } from "ai";
import { openai } from "@ai-sdk/openai";
import { z } from "zod";

const { partialObjectStream } = streamObject({
  model: openai("gpt-4.1"),
  schema: z.object({
    title: z.string(),
    date: z.string(),
    attendees: z.array(z.string()),
  }),
  prompt: "Alice and Bob meet Friday for lunch.",
});

for await (const partial of partialObjectStream) {
  // partial is a deep-Partial<T>: { title?: ..., attendees?: [...] }
  render(partial);                 // UI updates as each field arrives
}
```

```python
# Python — manual partial parse over a text stream
import json
from json_repair import repair_json     # tolerant parser

buf = ""
for chunk in client.chat.completions.create(
    model="gpt-4.1",
    messages=[{"role": "user", "content": "Alice and Bob meet Friday for lunch."}],
    response_format={"type": "json_schema", "json_schema": {...}},
    stream=True,
):
    delta = chunk.choices[0].delta.content
    if delta:
        buf += delta
        partial = repair_json(buf, return_objects=True)   # best-effort object so far
        render(partial)
```

### What a UI consumes

Consumers typically want a stream of progressively-more-complete objects where
every key is *optional* until the stream ends — a "deep partial" of the target type.
Rendering code reads `partial.title ?? "…"`, shows array items as they append, and
flips to a "done" state on the final, fully-validated object. The same incremental
parser underlies field-level streaming (whole fields at a time) and char-level
streaming (typewriter effect inside a field).

### What varies across providers

- The **wire stream is identical** to plain token streaming — partial structured
  output is a *client-side* interpretation of the text deltas, not a distinct API
  mode. So it works on any provider that streams text.
- Providers that stream tool-call arguments (OpenAI `tool_calls` deltas, Anthropic
  `input_json_delta`, Gemini function-call parts) give the *tool-injection* path to
  structured output its own incremental argument stream to parse the same way.
- SDKs differ on whether they hand you a typed deep-partial (Vercel `streamObject`)
  or leave you to parse the buffer yourself (raw SDKs).

### What's hard

- **Parsing invalid JSON on every delta** without crashing — and doing it cheaply
  enough to run per token.
- **Stable partial shapes.** A field that appears then changes type mid-stream
  (number → string) can flicker; the parser must emit something coherent each step.
- **Validation timing.** You cannot enforce `required` fields or coercion until the
  object is complete, so the partial type and the final type differ.

---

## ★ Multimodal input

**Goal:** "I want to send an image / audio / PDF along with my text and ask about it."

### How it's done today

Multimodal input rides on the same message array — `content` becomes a list of parts,
some text, some media. The media can be supplied as a **URL**, as **inline base64
bytes**, or as a **file/handle reference** to something uploaded earlier. Each
provider encodes these differently.

```python
# Python — OpenAI Chat Completions, image by URL and inline audio
resp = client.chat.completions.create(
    model="gpt-4.1",
    messages=[{
        "role": "user",
        "content": [
            {"type": "text", "text": "What's in this image?"},
            {"type": "image_url", "image_url": {"url": "https://example.com/cat.jpg", "detail": "auto"}},
            # base64 inline: {"url": "data:image/png;base64,iVBORw0..."}
            {"type": "input_audio", "input_audio": {"data": b64_wav, "format": "wav"}},
        ],
    }],
)
```

```python
# Python — Anthropic image (base64) + PDF document
msg = client.messages.create(
    model="claude-sonnet-4-5",
    max_tokens=1024,
    messages=[{
        "role": "user",
        "content": [
            {"type": "text", "text": "Summarize this document."},
            {"type": "image", "source": {"type": "base64", "media_type": "image/png", "data": b64_png}},
            # or url:  {"type": "image", "source": {"type": "url", "url": "https://..."}}
            {"type": "document", "source": {"type": "base64", "media_type": "application/pdf", "data": b64_pdf}},
        ],
    }],
)
```

```python
# Python — Gemini: inline bytes, plus a Files-API handle for large/video media
myfile = client.files.upload(file="lecture.mp4")     # returns a file handle/URI
resp = client.models.generate_content(
    model="gemini-2.5-flash",
    contents=[
        "Describe what happens in this video, then read this chart.",
        myfile,                                        # file handle
        genai.types.Part.from_bytes(data=png_bytes, mime_type="image/png"),  # inline
    ],
)
```

```ts
// TS — OpenAI Chat Completions, image URL
await client.chat.completions.create({
  model: "gpt-4.1",
  messages: [{
    role: "user",
    content: [
      { type: "text", text: "What's in this image?" },
      { type: "image_url", image_url: { url: "https://example.com/cat.jpg" } },
    ],
  }],
});
```

```ts
// TS — Anthropic base64 image
await client.messages.create({
  model: "claude-sonnet-4-5",
  max_tokens: 1024,
  messages: [{
    role: "user",
    content: [
      { type: "text", text: "What's in this image?" },
      { type: "image", source: { type: "base64", media_type: "image/jpeg", data: b64 } },
    ],
  }],
});
```

```ts
// TS — Gemini inline + uploaded file
import { GoogleGenAI, createUserContent, createPartFromUri } from "@google/genai";
const ai = new GoogleGenAI({});
const file = await ai.files.upload({ file: "lecture.mp4" });
await ai.models.generateContent({
  model: "gemini-2.5-flash",
  contents: createUserContent([
    "Describe the video.",
    createPartFromUri(file.uri, file.mimeType),
  ]),
});
```

### What varies across providers

| | OpenAI Chat | Anthropic | Gemini |
|---|---|---|---|
| Image part | `image_url` (URL or `data:` base64) | `image` with `source` (`base64`/`url`) | `inline_data` (base64) or `file_data` (URI) |
| Base64 framing | `data:<mime>;base64,...` data-URL | raw base64 + explicit `media_type` | raw base64 + explicit `mime_type` |
| File handles | OpenAI file ids (Responses `input_file`) | n/a (inline/URL) | Files API URI; `gs://` URIs |
| Audio input | `input_audio` (wav/mp3, inline) | not supported | supported (inline / file) |
| Video | no | no | yes (file handle) |
| PDFs | via file id / Responses | `document` part (base64/url) | as a file or inline |
| Detail/resolution hint | `detail: low/high/auto` | — | — |

### What's hard

- **Three encodings for "an image."** URL vs data-URL vs raw-base64-plus-mime-type
  vs uploaded-handle — and which ones a provider accepts varies by media type.
- **Pre-fetch / pre-upload decisions.** If a provider can't ingest a given URL, the
  caller must download the bytes and inline them, or upload to a Files API first and
  reference the handle. (The Vercel AI SDK encodes this as a per-model `supportedUrls`
  capability map and downloads when the URL doesn't match.)
- **Capability gaps must be detected, not assumed.** Sending audio to a provider that
  doesn't accept it has to be caught and either rejected or degraded (e.g. transcribe
  first) rather than silently dropped.
- **Size limits and base64 bloat.** Inlining large files inflates the request ~33%
  and can blow request-size limits, which is exactly why Files-API handles exist.

---

## ◆ Non-text output

**Goal:** "I want the model to produce an image, speech, or a transcript — not text."

### How it's done today

Two shapes coexist, and they are **converging**. The older shape is a *dedicated,
single-purpose endpoint*; the newer — and increasingly the default — shape is the
*same conversation API* emitting non-text output as a **built-in tool** or a
**native output modality**. "Non-text output = a separate API" is the legacy mental
model; today it's often just another thing the main multimodal call can return.

**(a) Dedicated endpoints — one-shot, you name the media model directly.** Best when
you want a single asset from a single prompt.

```python
# Python — OpenAI dedicated endpoints
img = client.images.generate(model="gpt-image-2", prompt="a watercolor fox", size="1024x1024")
b64_png = img.data[0].b64_json

speech = client.audio.speech.create(model="gpt-4o-mini-tts", voice="alloy", input="Hello there.")
speech.stream_to_file("hello.mp3")

with open("hello.mp3", "rb") as f:
    text = client.audio.transcriptions.create(model="gpt-4o-transcribe", file=f).text
```

```ts
// TS — OpenAI dedicated endpoints (TTS + STT)
const speech = await client.audio.speech.create({
  model: "gpt-4o-mini-tts", voice: "alloy", input: "Hello there.",
});
const buf = Buffer.from(await speech.arrayBuffer());

import fs from "node:fs";
const tx = await client.audio.transcriptions.create({
  model: "gpt-4o-transcribe", file: fs.createReadStream("hello.mp3"),
});
console.log(tx.text);
```

**(b) In the conversation API — non-text output in context.** The same multimodal
request that produces text now returns images/audio too, so generation composes with
reasoning, tools, and multi-turn history. OpenAI exposes image generation as a
**built-in tool**; the image comes back as an `image_generation_call` *output item* —
the same typed-item model as everything else in Responses — and a *mainline* model
(e.g. `gpt-5.5`) picks the underlying GPT-Image model.

```python
# Python — OpenAI Responses: image generation as a built-in tool, edited across turns
resp = client.responses.create(
    model="gpt-5.5",
    input="Generate an image of a gray tabby cat hugging an otter with an orange scarf",
    tools=[{"type": "image_generation"}],
)
png_b64 = next(o.result for o in resp.output if o.type == "image_generation_call")

edit = client.responses.create(           # iterate in the same conversation
    model="gpt-5.5",
    previous_response_id=resp.id,
    input="Now make it look realistic",
    tools=[{"type": "image_generation"}],   # action: auto | generate | edit
)
```

```ts
// TS — partial-image streaming from the same call
const stream = await client.responses.create({
  model: "gpt-5.5",
  input: "Draw a river made of white owl feathers in a winter landscape",
  stream: true,
  tools: [{ type: "image_generation", partial_images: 2 }],
});
for await (const e of stream) {
  if (e.type === "response.image_generation_call.partial_image")
    save(`partial-${e.partial_image_index}.png`, e.partial_image_b64);
}
```

Gemini goes further toward native: generated images (and audio) come back as
`inline_data` **parts on a normal `generateContent` response** — never a separate
call.

```python
# Python — Gemini: image output is just another content part
resp = client.models.generate_content(model="gemini-2.5-flash-image", contents="a watercolor fox")
for part in resp.candidates[0].content.parts:
    if part.inline_data:                       # image bytes arrive inline
        png = part.inline_data.data
```

**Audio output** follows the same trajectory: speech-capable chat models and the
Realtime API (see [`04-realtime-and-transports.md`](04-realtime-and-transports.md))
emit audio *within* the response/stream rather than through a separate TTS endpoint.

### What varies across providers

- **Separate endpoint vs in-conversation.** OpenAI offers *both* — dedicated
  `images` / `audio.*` endpoints **and** an `image_generation` Responses tool. Gemini
  leans native-inline (content parts). The dedicated endpoint suits one-shot single
  assets; the in-conversation path suits multi-turn, editable, context-aware
  generation. The direction of travel is toward the in-conversation path.
- **Who picks the media model.** Dedicated: you name the image/TTS model directly.
  In-conversation: you name a *mainline* model and the tool selects the media
  sub-model — and usage bills mainline tokens **plus** generation cost.
- **Return encoding.** base64 vs hosted URL vs an inline content part vs a typed
  output item; audio as streamed bytes; STT as text (± word/segment timestamps).
- **Streaming.** TTS streams audio; STT streams partial text; image-gen can stream
  **partial preview images** (`partial_images`, 0–3), in *both* the dedicated and the
  Responses path.
- **Realtime** collapses STT → LLM → TTS into one bidirectional audio stream
  (see [`04-realtime-and-transports.md`](04-realtime-and-transports.md)).

### What's hard

- **Output isn't a string.** The result is bytes (or a hosted URL, or a typed output
  item) with a mime type — a text-shaped abstraction doesn't fit; you need a media
  result type carrying mime + data/URL.
- **Two paths, two cost meters, two extraction shapes.** The same capability via a
  dedicated endpoint vs an in-conversation tool differs in request shape, result
  extraction (`data[0].b64_json` vs an `image_generation_call` output item vs an
  inline part), and billing (the tool adds mainline-model tokens on top of generation
  cost). An abstraction has to paper over both.
- **Mixed-modality responses.** Once a single response can interleave text, image
  parts, generation-call items, and reasoning, every response parser must handle media
  in an otherwise-textual reply.
- **Generation-specific failure modes.** Long latency (image gen up to ~2 min),
  prompt moderation with its own error shape, and provider-*revised* prompts
  (`revised_prompt`) that differ from what you sent.

---

## ◆ Grounding / citations as model output

**Goal:** "When the model answers from documents I provide (or from search), I want
it to tell me *which source* backs each claim — spans, not just prose."

### How it's done today

This is purely about the **shape of what the model returns**, not retrieval
architecture: given source material in the request, some providers emit *structured
citations* — pointers (document index, character/page span, the quoted text)
attached to the parts of the answer they support. The RAG pipeline that *found* the
documents is out of scope (see [`02`](02-tools-and-agents.md) for hosted
file/web-search tools); this is the call surface that turns a sourced answer into
machine-readable citations.

**Anthropic — Citations API.** Set `citations: {enabled: true}` on a document
content block; the response's `text` blocks then carry a `citations` array, each
entry a `char_location` / `page_location` with `cited_text`, `document_index`, and
span indices. `cited_text` does not count toward output tokens.

```python
# Python — Anthropic citations: enable per document, read spans off the answer
msg = client.messages.create(
    model="claude-sonnet-4-5",
    max_tokens=1024,
    messages=[{
        "role": "user",
        "content": [
            {
                "type": "document",
                "source": {"type": "text", "media_type": "text/plain",
                           "data": "The grant deadline is March 14, 2026..."},
                "title": "Grant FAQ",
                "citations": {"enabled": True},      # opt in per document
            },
            {"type": "text", "text": "When is the grant deadline?"},
        ],
    }],
)
for block in msg.content:
    if block.type == "text":
        print(block.text)
        for c in (block.citations or []):
            # char_location: cited_text, document_index, document_title,
            #                start_char_index, end_char_index
            print("  ↳ source", c.document_index, repr(c.cited_text))
```

**Gemini — grounding metadata.** When grounded (e.g. against the Google Search tool
or supplied data), the candidate carries a `groundingMetadata` object with
`groundingChunks` (the sources) and `groundingSupports` (which answer segments map
to which chunks, with text-span ranges and confidence).

```python
# Python — Gemini grounding metadata on the candidate
resp = client.models.generate_content(
    model="gemini-2.5-flash",
    contents="Who won the 2026 Best Picture Oscar?",
    config={"tools": [{"google_search": {}}]},
)
gm = resp.candidates[0].grounding_metadata
for chunk in gm.grounding_chunks:                # the cited sources (web/retrieved)
    print(chunk.web.uri, chunk.web.title)
for sup in gm.grounding_supports:                # answer span → source indices
    print(sup.segment.text, "→", sup.grounding_chunk_indices)
```

```ts
// TS — Anthropic citations: enabled per document block
const msg = await client.messages.create({
  model: "claude-sonnet-4-5",
  max_tokens: 1024,
  messages: [{
    role: "user",
    content: [
      {
        type: "document",
        source: { type: "text", media_type: "text/plain", data: "The grant deadline is March 14, 2026..." },
        title: "Grant FAQ",
        citations: { enabled: true },
      },
      { type: "text", text: "When is the grant deadline?" },
    ],
  }],
});
for (const block of msg.content) {
  if (block.type === "text" && block.citations) {
    for (const c of block.citations) console.log(c.document_index, c.cited_text);
  }
}
```

### What varies across providers

- **Whether structured citations exist at all.** Anthropic has a first-class
  Citations API (opt-in per document, spans into *your* provided documents). Gemini
  emits `groundingMetadata` (chunks + supports), strongest when grounded against
  Search; Vertex exposes the same grounding shape (plus enterprise grounding to your
  data stores). **OpenAI** surfaces sources via *annotations* on web-search/file-search
  tool output (URL citations / file-citation spans) rather than a per-document
  citations toggle — so "sourced answer" exists on all three, but the trigger and the
  return shape differ entirely.
- **What the span points at.** Anthropic points into the documents *you* supplied
  (char/page/block index + `cited_text`). Gemini's `groundingSupports` point answer
  segments at `groundingChunks` (often web URIs). The unit of citation (your doc vs a
  retrieved chunk) is not the same object.
- **Where it lives in the response.** Anthropic: a `citations` array *inside each text
  block*. Gemini: a `groundingMetadata` object *beside* the content on the candidate.
  Extraction is structurally different.

### What's hard

- **No shared citation type.** A span-into-my-doc (Anthropic), an answer-segment→
  web-chunk map (Gemini), and a tool-output annotation (OpenAI) have to be normalized
  into one "claim → source span" shape, in both directions, to render uniform footnotes.
- **Coverage is partial.** Not every sentence gets a citation, and a claim may cite
  several spans or none — UI has to handle answer text that is only partially grounded.
- **Trigger differs from output.** On Gemini/OpenAI citations only appear when a
  grounding/search tool actually fired; on Anthropic they appear only for documents
  you flagged — so "ask for citations" is not one switch across providers.

---

## ◆ Reasoning models

**Goal:** "I want the model to think hard before answering — and I want to see, pay
for, and preserve that thinking correctly."

### How it's done today

Reasoning models (OpenAI o-series / GPT-5 reasoning, Anthropic extended thinking,
Gemini "thinking") spend extra hidden tokens deliberating before the visible answer.
You control how much, and you may get a *summary* of the reasoning back.

```python
# Python — OpenAI Responses, reasoning effort + summary
resp = client.responses.create(
    model="o4-mini",
    input="Prove there are infinitely many primes.",
    reasoning={"effort": "high", "summary": "auto"},   # low | medium | high
)
print(resp.output_text)
# reasoning tokens billed under usage.output_tokens_details.reasoning_tokens
```

```python
# Python — Anthropic extended thinking (budget in tokens)
msg = client.messages.create(
    model="claude-sonnet-4-5",
    max_tokens=4096,
    thinking={"type": "enabled", "budget_tokens": 2048},
    messages=[{"role": "user", "content": "Prove there are infinitely many primes."}],
)
for block in msg.content:
    if block.type == "thinking":
        print("THINKING:", block.thinking)        # a thinking content block
    elif block.type == "text":
        print("ANSWER:", block.text)
```

```python
# Python — Gemini thinking config
resp = client.models.generate_content(
    model="gemini-2.5-pro",
    contents="Prove there are infinitely many primes.",
    config={"thinking_config": {"thinking_budget": 2048, "include_thoughts": True}},
)
```

```ts
// TS — Anthropic extended thinking
const msg = await client.messages.create({
  model: "claude-sonnet-4-5",
  max_tokens: 4096,
  thinking: { type: "enabled", budget_tokens: 2048 },
  messages: [{ role: "user", content: "Prove there are infinitely many primes." }],
});
```

### ◆ Output-control levers: effort & verbosity

Newer OpenAI reasoning models expose two *independent* knobs that shape the output
rather than its content. They are separate concerns and tune separately.

- **`reasoning.effort`** — how much the model *thinks* before answering. For gpt-5.5
  the values are `none`, `low`, `medium`, `high`, `xhigh` (default `medium`). Lower =
  faster and cheaper; higher = more planning, debugging, and synthesis. The right
  value depends on the task: `low` for extraction / routing / classification;
  `medium`–`high` for diagnosis, planning, and code; `xhigh` only when evals justify
  the added latency. This is the same idea other providers expose differently — e.g.
  Anthropic's extended-thinking *token budget* (above).
- **`text.verbosity`** — `low` / `medium` / `high`; the main lever for brevity vs
  completeness, *independent* of effort. Lower verbosity = fewer output tokens and a
  faster response. Effort controls *thinking*; verbosity controls *answer length* —
  two separate knobs, not one.

```python
# Python — OpenAI Responses, effort + verbosity
resp = client.responses.create(
    model="gpt-5.5",
    input="Diagnose why this build is flaky.",
    reasoning={"effort": "high"},        # none | low | medium | high | xhigh
    text={"verbosity": "low"},           # low | medium | high
)
print(resp.output_text)
```

```ts
// TS — OpenAI Responses, effort + verbosity
const resp = await client.responses.create({
  model: "gpt-5.5",
  input: "Diagnose why this build is flaky.",
  reasoning: { effort: "high" },         // none | low | medium | high | xhigh
  text: { verbosity: "low" },            // low | medium | high
});
console.log(resp.output_text);
```

### What varies across providers

- **Effort knob.** OpenAI uses a categorical `effort` (`low`/`medium`/`high`);
  Anthropic and Gemini use a token *budget*. Some agent runtimes expose extra rungs
  (`xhigh`/`max`).
- **What you get back.** OpenAI returns *summaries* of reasoning (`summary: auto/
  concise/detailed`) as `reasoning` output items, and can return **encrypted
  reasoning** that you can't read but can pass back. Anthropic returns full `thinking`
  content blocks (with cryptographic signatures). Gemini returns optional "thought"
  parts. The raw chain-of-thought is generally not fully exposed.
- **Billing.** Reasoning tokens are billed as output tokens, often broken out
  (`output_tokens_details.reasoning_tokens`). They consume the output budget.
- **Constraints.** Some reasoning models restrict or ignore `temperature`, require
  the Responses API rather than Chat Completions, or change `max_tokens` semantics.
- **Effort/verbosity naming and granularity diverge.** The number and names of effort
  rungs differ (OpenAI gpt-5.5 `none`/`low`/`medium`/`high`/`xhigh`; others use a token
  budget; some runtimes add `max`), and an explicit *verbosity* knob is not universal —
  several APIs conflate "how much to think" and "how long the answer is" into a single
  control or leave answer length to the prompt, so the two-knob model doesn't map cleanly.

### Why continuity across turns matters

Reasoning content is **stateful**. To preserve a chain of thought across turns you
must feed prior reasoning back in:

- Anthropic requires sending the original `thinking` blocks (with their signatures)
  back in the assistant turn when tools are involved — dropping or editing them breaks
  the next turn.
- OpenAI's Responses API preserves reasoning **server-side** when you chain with
  `previous_response_id`; if you instead resend history yourself, you must include the
  encrypted reasoning items, or the model loses its train of thought.
- Translating a transcript *between* providers is lossy: Anthropic `thinking` blocks
  have no Gemini equivalent and are dropped; encrypted OpenAI reasoning is opaque to
  everyone else.

### What's hard

- **Reasoning is a distinct content kind**, not text — it must be modeled separately
  so it can be displayed, billed, stripped, or round-tripped without contaminating
  the answer.
- **Lossless round-tripping** (signatures, encrypted blobs, server-side state) is
  required for correctness on the *next* turn, which makes reasoning continuity a
  cross-turn concern even in a "single-turn" file.
- **Two incompatible control models** (effort category vs token budget) have to be
  reconciled by any abstraction that wants one knob.

---

## ★ Tokens & limits

**Goal:** "I need to control output length, stay inside the context window, count
tokens before sending, and know what I was charged."

### How it's done today

**Max output tokens.** Every API caps how much the model may *generate* (separate
from the context window). On Anthropic `max_tokens` is required; elsewhere it's
optional with a model default.

```python
# OpenAI Chat: max_tokens (newer reasoning models: max_completion_tokens)
client.chat.completions.create(model="gpt-4.1", messages=[...], max_tokens=512)
# Anthropic: required
client.messages.create(model="claude-sonnet-4-5", max_tokens=512, messages=[...])
# Gemini: maxOutputTokens
client.models.generate_content(model="gemini-2.5-flash", contents="...",
                               config={"max_output_tokens": 512})
```

**Token counting** before you send, to fit the context window or estimate cost:

```python
# Python — OpenAI, local tokenizer
import tiktoken
enc = tiktoken.encoding_for_model("gpt-4.1")
n = len(enc.encode("How many tokens is this?"))

# Python — Anthropic, server-side exact count
ct = client.messages.count_tokens(
    model="claude-sonnet-4-5",
    messages=[{"role": "user", "content": "How many tokens is this?"}],
)
print(ct.input_tokens)

# Python — Gemini, server-side
print(client.models.count_tokens(model="gemini-2.5-flash", contents="...").total_tokens)
```

**Usage reporting** comes back on the response:

```python
u = resp.usage                          # OpenAI: prompt_tokens, completion_tokens, total_tokens
u = msg.usage                           # Anthropic: input_tokens, output_tokens, cache_*_input_tokens
u = resp.usage_metadata                 # Gemini: prompt_token_count, candidates_token_count, total_token_count
```

```ts
// TS — usage shapes
resp.usage;            // OpenAI:    { prompt_tokens, completion_tokens, total_tokens }
msg.usage;             // Anthropic: { input_tokens, output_tokens, cache_creation_input_tokens, cache_read_input_tokens }
resp.usageMetadata;    // Gemini:    { promptTokenCount, candidatesTokenCount, totalTokenCount, cachedContentTokenCount }
```

### What varies across providers

| Concern | OpenAI | Anthropic | Gemini |
|---|---|---|---|
| Output cap field | `max_tokens` / `max_completion_tokens` | `max_tokens` (required) | `maxOutputTokens` |
| Usage field names | `prompt_`/`completion_`/`total_tokens` | `input_`/`output_tokens` + cache | `*_token_count` |
| Counting | local (`tiktoken`) | server (`count_tokens`) | server (`count_tokens`) |
| Cache accounting | `cached_tokens` detail | `cache_creation`/`cache_read` tokens | `cached_content_token_count` |
| Reasoning tokens | `reasoning_tokens` detail | counted in output | "thoughts" tokens |
| Truncation signal | `finish_reason: "length"` | `stop_reason: "max_tokens"` | `finishReason: "MAX_TOKENS"` |
| Context window | per model | per model | per model |

### What's hard

- **Usage field names don't line up** (`prompt`/`input`, `completion`/`output`,
  nested `*_details` vs top-level), so cost/telemetry code must normalize three
  shapes — plus cache and reasoning sub-counts.
- **Counting is inconsistent.** OpenAI can be counted locally; Anthropic and Gemini
  require a server round-trip (or an estimate), and multimodal inputs (images, audio)
  have their own token math that local tokenizers don't capture.
- **Truncation is a silent correctness bug.** Hitting the output cap returns a
  partial answer with a distinct stop reason per provider — structured output then
  fails to parse, and the only signal is the (differently-named) finish reason.
- **Cap semantics differ.** For reasoning models the output cap must also cover hidden
  reasoning tokens, so a budget that worked on a non-reasoning model can truncate the
  visible answer to nothing.

---

## What varies / what's hard (single-turn summary)

Even for the simplest possible call — one prompt in, one answer out — the three major
APIs diverge on nearly every axis:

- **Message model.** `messages[]` (OpenAI, Anthropic) vs `contents[]` with `parts`
  (Gemini) vs a flat `input` *item* list (OpenAI Responses). Roles disagree:
  `assistant` vs `model`, `tool` vs `function`, `system` as a message vs a top-level
  field vs `developer`.
- **Structured output.** One desired type → four wire formats: `response_format`,
  `text.format`, forced tool injection, and `responseSchema` — and a *different schema
  dialect* for Gemini. Strict guarantees, fallbacks, and `json_object` support all
  differ; a coercion/repair layer is needed under all of them.
- **Streaming transport.** SSE (OpenAI, Anthropic, with two different event grammars)
  vs chunked JSON arrays (Gemini). Deltas vs full snapshots; different finish signals;
  different usage timing. Partial-structured streaming is a client-side parse on top
  of whichever stream you get.
- **Multimodal encoding.** URL vs data-URL vs raw-base64+mime vs uploaded file handle,
  with per-provider, per-media-type capability gaps (audio: OpenAI/Gemini yes,
  Anthropic no; video: Gemini only).
- **Non-text output** is converging into the main multimodal API — a built-in tool
  (OpenAI `image_generation`) or a native inline modality (Gemini content parts,
  audio-out/Realtime) — while dedicated endpoints survive for one-shot single assets.
  Two paths, two billing/extraction shapes to reconcile.
- **Reasoning** uses an effort category vs a token budget, returns summaries vs full
  thinking blocks vs encrypted/opaque state, and must be round-tripped *correctly* to
  preserve continuity across turns.
- **Tokens & limits.** Required vs optional output caps, three usage shapes, local vs
  server-side counting, and per-provider truncation signals.

The recurring theme: there is **no shared type for a message, a response, a stream
event, a piece of media, a unit of usage, or a schema**. Anything that wants to speak
to more than one provider has to define a canonical shape for each and pay the
translation cost — in both directions — at the boundary.
