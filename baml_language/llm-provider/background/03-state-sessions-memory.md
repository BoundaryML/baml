# 03 — Making it remember

*Within-run history, sessions, server-stored chains, ownership models, and memory.*

> Legend: `★ table-stakes` · `◆ advanced` · `▲ frontier`

A single model call is stateless. The endpoint takes whatever you send and
returns one answer; it remembers nothing about the last call. Everything that
feels like "the assistant remembers what I said" is an illusion built on top of
that stateless call — by *resending* prior turns, or by *storing* them somewhere
and *retrieving* them on the next call.

This file maps the spectrum of how that illusion is built, from the simplest
(rebuild and resend the whole transcript every turn) to the most elaborate
(a curated, deduplicated fact store that survives across every conversation a
user ever has). The territory splits into two genuinely different problems:

- **Sessions** — the transcript of *one* conversation. Append-only, verbatim,
  identified by a session id, lifetime of hours to days.
- **Memory** — distilled facts about a *user* or *agent* that persist across
  *all* their conversations. Mutable, deduplicated, retrieved by relevance,
  lifetime of months to years.

Different lifetimes, different storage, different retrieval. Almost every mature
SDK eventually grew separate APIs for each. The last two sections lay out that
distinction explicitly.

The capabilities of a single call live in
[`01-single-turn.md`](01-single-turn.md); the tool loop and multi-agent
orchestration that *consume* this state live in
[`02-tools-and-agents.md`](02-tools-and-agents.md). Server-side sessions that
exist only inside a live connection (Realtime/voice) are covered in
[`04-realtime-and-transports.md`](04-realtime-and-transports.md); caching as a
state-lifecycle concern is in [`05-cross-cutting.md`](05-cross-cutting.md); and
the runtimes that package sessions/memory into deployable agents are in
[`06-harnesses.md`](06-harnesses.md).

---

## 1. ★ Within-run history — the baseline

**Goal.** *"I want the model to see what was said earlier in this
conversation, so its next answer is in context."*

### How it's done today

The baseline mechanism, and the one every other technique is layered on top of,
is dead simple: the conversation is an **array of messages**, you **append** the
latest user and assistant turns to it, and you **resend the whole array** on
every call. The model is stateless; the array *is* the memory.

```python
# Python — OpenAI (Chat Completions)
from openai import OpenAI
client = OpenAI()

messages = [
    {"role": "system", "content": "You are a terse assistant."},
]

def ask(user_text: str) -> str:
    messages.append({"role": "user", "content": user_text})
    resp = client.chat.completions.create(model="gpt-4o", messages=messages)
    answer = resp.choices[0].message.content
    messages.append({"role": "assistant", "content": answer})   # keep for next turn
    return answer

ask("What's the capital of France?")
ask("And its population?")   # "its" only resolves because the array carries turn 1
```

```ts
// TS — Anthropic
import Anthropic from "@anthropic-ai/sdk";
const client = new Anthropic();

const system = "You are a terse assistant.";
const messages: Anthropic.MessageParam[] = [];

async function ask(userText: string): Promise<string> {
  messages.push({ role: "user", content: userText });
  const resp = await client.messages.create({
    model: "claude-sonnet-4-5",
    max_tokens: 1024,
    system,                       // system prompt is a top-level field, not a message
    messages,
  });
  const answer = resp.content[0].type === "text" ? resp.content[0].text : "";
  messages.push({ role: "assistant", content: answer });
  return answer;
}
```

Three things are worth naming because they recur everywhere downstream:

- **System prompt placement.** OpenAI Chat Completions puts the system prompt as
  the first element *inside* the `messages` array (`role: "system"`). Anthropic
  hoists it to a top-level `system` field that sits *outside* the array. The
  Responses API uses a top-level `instructions` field. Same intent, three
  positions — and any abstraction that normalizes messages has to decide where
  the system text lives.
- **The assistant turn must be appended too.** A common bug is appending only
  user turns; the model then never sees its own prior answers and contradicts
  itself. The full transcript is user *and* assistant turns interleaved.
- **Tool calls become messages.** Once tools enter (see
  [`02-tools-and-agents.md`](02-tools-and-agents.md)), the assistant's
  `tool_use` and your `tool_result` are *also* messages in the array. The
  transcript is the complete record of everything that happened, not just the
  prose.

### What varies across providers

- **Where the system prompt goes** — in-array (OpenAI Chat) vs. top-level field
  (Anthropic `system`, Responses `instructions`, Gemini `systemInstruction`).
- **The message grammar.** Chat Completions is a list of `{role, content}`
  messages. Gemini uses `contents` with `parts`. The Responses API `input` is a
  list of *typed items* — a union of `message`, `function_call`,
  `function_call_output`, `reasoning`, … (message items are still role-keyed and
  still nest a `content` parts array, so it's two-level, not flat). The novelty:
  the response's `output` items round-trip straight back into the next `input`,
  the same shapes flowing both directions.
- **What counts as "the assistant turn."** With reasoning models, the assistant
  turn may include reasoning/thinking blocks that some providers want echoed
  back and others drop (see §4 and [`01-single-turn.md`](01-single-turn.md)).

### What's hard

- **Unbounded growth.** The array grows every turn; so does cost and latency,
  because you resend everything each call. Eventually you hit the context
  window. Mitigations — windowing (keep the last *k* turns), summarizing older
  turns into a rolling synopsis, or token-budget trimming — are exactly the
  classic LangChain memory policies (`ConversationBufferWindowMemory`,
  `ConversationSummaryMemory`, `ConversationTokenBufferMemory`). None are free:
  windowing forgets, summarizing distorts.
- **It is the application's job to *hold* the array.** In the baseline above the
  array is an in-process variable; close the process and it is gone. Persisting
  it across requests/restarts is the entire point of the next section.
- **Reasoning continuity.** For o-series / thinking models, dropping the prior
  reasoning blocks between turns silently degrades multi-step performance — a
  problem that server-stored chains (§4) exist to solve.

---

## ◆ Compaction: shrinking the live context

**Goal.** *"My long-running run has accumulated old messages, tool logs,
retries, and stale details that crowd out the state the model actually needs. I
want to compress the window while preserving forward-looking state — without
hitting the context limit or going dumb."*

Where §1's "what's hard" names unbounded growth as the problem, compaction is one
concrete answer to it — but a distinct one. It is neither a session (the
verbatim transcript of §2) nor memory (the cross-conversation fact store of §6):
it is a provider-produced *replacement* for the live window itself, machine state
that stands in for the turns it summarizes.

### How it's done today

The headline mechanism is OpenAI Responses **compaction**. The provider takes a
long conversation and returns a smaller set of items that carries the same
forward-looking state. There are two modes.

**Server-managed.** With a server-stored chain (`previous_response_id`, §4), turn
on `context_management` with a `compact_threshold`. When the stored conversation
grows past the threshold, the server compacts it automatically on the next turn;
the client keeps doing exactly what it did before — sending only the newest user
message and the prior response id.

```python
# Python — OpenAI Responses: server-managed compaction on a stored chain
from openai import OpenAI
client = OpenAI()

r = client.responses.create(
    model="gpt-5",
    input="Start debugging the failing checkout flow.",
    context_management={"compact_threshold": 100_000},  # auto-compact past ~100k tokens
)
# ... many turns later, still just the new turn + the handle:
r = client.responses.create(
    model="gpt-5",
    input="Now check the refund path.",
    previous_response_id=r.id,             # server compacts its stored chain as needed
    context_management={"compact_threshold": 100_000},
)
```

**Manual.** When the client holds the window itself (client-held, §5), call
`compact()` explicitly. It returns a smaller window in `compacted.output`, which
you pass **straight into** the next `create()` ahead of the new user message.

```python
# Python — OpenAI Responses: manual compaction of a client-held window
from openai import OpenAI
client = OpenAI()

# `long_window` is the accumulated list of input items (messages, tool calls,
# tool outputs, reasoning items) that has grown too large.
compacted = client.responses.compact(model="gpt-5", input=long_window)

# compacted.output is a SMALLER list of items — machine state, not prose.
# Pass it forward verbatim, then append the next user turn.
r = client.responses.create(
    model="gpt-5",
    input=[*compacted.output, {"role": "user", "content": "What's the root cause?"}],
)
```

```ts
// TS — OpenAI Responses: manual compact() → create()
import OpenAI from "openai";
const client = new OpenAI();

const compacted = await client.responses.compact({ model: "gpt-5", input: longWindow });

const r = await client.responses.create({
  model: "gpt-5",
  // spread the compacted items as-is, then add the new turn
  input: [...compacted.output, { role: "user", content: "What's the root cause?" }],
});
```

Two things make compaction its own primitive rather than "summarization with a
nicer API":

- **The output is machine state, not a human summary.** `compacted.output` is a
  list of typed items in the same grammar the model round-trips (§1) — it may fold
  reasoning state and tool history into a compressed form a human would not write
  by hand. **Do not edit it.** Pass it forward exactly as returned, then add the
  next user message. Editing or reordering it corrupts the state the next turn
  depends on.
- **Timing is a choice the caller makes (in manual mode).** A good moment to
  compact is *after a milestone* — a debugging phase finishes, a root cause
  narrows, a sub-task completes — when the detail that got you here is no longer
  load-bearing but the conclusion is.

### What varies across providers

Compaction is distinct from the two adjacent techniques this file already
covers, and from each other:

- **vs. rolling-summary memory (§6).** A summary memory is *LLM-written prose* —
  "the user is debugging checkout; the root cause is a null `cart_id`." It is
  human-readable, hand-editable, and you control the prompt that produces it.
  Compaction output is *provider-produced machine state* you are told not to
  touch.
- **vs. plain truncation / windowing (§1).** Truncation drops old turns
  wholesale (keep the last *k*); windowing slides a fixed frame. Both are lossy in
  a blunt, caller-controlled way and require no provider support. Compaction is
  lossy in a *model-controlled* way — the provider decides what state to keep.
- **Anthropic Messages offers a different shape: server-side context editing.**
  Anthropic does not expose OpenAI's "return a smaller item list" `compact()`
  primitive, but it does ship first-class, server-side context management — so the
  earlier framing that teams there must roll their own is no longer accurate. The
  `context_management` parameter (beta header `context-management-2025-06-27`)
  takes an `edits` array of *clearing strategies* the server applies in place:
  `clear_tool_uses_20250919` drops old tool-use/tool-result blocks once a token
  trigger is crossed, and `clear_thinking_20251015` drops older thinking blocks.
  Each strategy has `trigger`/`keep` knobs (e.g. keep the last *N* tool uses, fire
  past 100k input tokens) and the response reports what it removed in an
  `applied_edits` field. The mental model differs from OpenAI compaction: this
  *prunes* stale blocks rather than *summarizing* them into replacement state —
  closer to server-enforced windowing than to a summary. (Anthropic also has a
  separate server-side **compaction** primitive — `compact_20260112`, beta header
  `compact-2026-01-12` — that *does* summarize earlier turns into a `compaction`
  block once the window fills, which you must echo back on the next request. The
  two compose: clear tool noise as you go, summarize the rest near the limit.)

  ```python
  # Python — Anthropic: server-side context editing clears stale tool/thinking blocks
  resp = client.beta.messages.create(
      model="claude-sonnet-4-5",
      max_tokens=1024,
      betas=["context-management-2025-06-27"],
      messages=messages,                       # long, tool-heavy transcript
      context_management={
          "edits": [
              # clear_thinking must precede clear_tool_uses when combined
              {"type": "clear_thinking_20251015", "keep": {"type": "thinking_turns", "value": 2}},
              {"type": "clear_tool_uses_20250919",
               "trigger": {"type": "input_tokens", "value": 50_000},
               "keep": {"type": "tool_uses", "value": 3}},
          ]
      },
  )
  # resp.context_management.applied_edits reports what the server removed this turn
  ```

- **Gemini `generateContent` still has no compaction endpoint.** Teams there roll
  their own out of the §1/§6 toolkit — windowing, truncation, or an LLM-written
  rolling summary. The framework memory classes already named in §1 are exactly
  this: LangChain's `ConversationSummaryMemory` /
  `ConversationSummaryBufferMemory` and LlamaIndex's summary-buffer memories are
  hand-rolled compaction — an LLM rewrites the old turns into a synopsis once a
  token budget is crossed. The difference is ownership: those produce prose you
  own and can inspect; provider compaction/context-editing produces opaque state
  you cannot.

### What's hard

- **Deciding *when* to compact.** Too early and you discard detail a later turn
  needed; too late and you hit the context limit anyway (see
  [`01-single-turn.md`](01-single-turn.md) on token limits). Server-managed mode
  hides this behind a threshold; manual mode makes it the caller's judgment call,
  best tied to a task milestone rather than a raw token count.
- **Trusting opaque machine state.** You cannot read `compacted.output` and
  verify it kept the right things — it is not prose. You are trusting the provider
  to preserve forward-looking state, with no inspection seam.
- **Interaction with reasoning items and caching.** Compaction has to round-trip
  reasoning items correctly (the same continuity concern as §1/§4); and because it
  produces a *new prefix*, it changes the prompt-cache key — the turn right after a
  compaction pays a cache miss on everything before the new boundary (see
  [`05-cross-cutting.md`](05-cross-cutting.md) on caching).
- **Lossy by design.** A bad compaction can silently drop a detail a later turn
  depended on, and because the dropped turns are gone from the live window, the
  failure surfaces as the model "forgetting" something it clearly knew earlier.
  This is the same hazard as windowing, moved inside the provider.

The runtimes that drive long-running agents — and therefore have to decide when
and how to compact across many turns — are covered in
[`07-workflows-and-orchestration.md`](07-workflows-and-orchestration.md).

---

## 2. ★ Sessions (transcripts)

**Goal.** *"I want to persist this conversation under an id, then resume or
continue it later — across requests, restarts, or machines."*

### How it's done today

A **session** is the within-run array of §1, given an **id** and a **storage
backend**, with operations to **append** new items and **read** them back. The
SDK (or your code) loads the transcript on the way in and saves it on the way
out. The transcript *is* the session.

```python
# Python — OpenAI Agents SDK: SQLiteSession
from agents import Agent, Runner, SQLiteSession

agent = Agent(name="Assistant", instructions="Be helpful.")
session = SQLiteSession("user-42-thread-1", db_path="conversations.db")  # file-backed

# Runner prepends session.get_items() to input, appends new items after the run.
await Runner.run(agent, "Book me a flight to Tokyo", session=session)
await Runner.run(agent, "Make it business class", session=session)  # remembers turn 1
```

The OpenAI Agents `Session` is a tiny protocol — four methods — and `Runner.run`
wraps every call with a load/save around it:

```python
# Python — the Session protocol (OpenAI Agents SDK)
class Session(Protocol):
    session_id: str
    async def get_items(self, limit: int | None = None) -> list[TResponseInputItem]: ...
    async def add_items(self, items: list[TResponseInputItem]) -> None: ...
    async def pop_item(self) -> TResponseInputItem | None: ...   # e.g. undo last turn
    async def clear_session(self) -> None: ...
```

`SQLiteSession(session_id, db_path=None)` is in-memory when `db_path` is `None`,
persistent otherwise. To swap backends you implement the four methods against
Redis, Postgres, whatever — the agent loop is untouched.

The Claude Agent SDK takes the same idea but makes the **filesystem** the
default store: sessions are **JSONL files** on disk, one event per line, under
`~/.claude/projects/<encoded-cwd>/*.jsonl`. There is no separate metadata
record — the transcript file *is* the session. Three ways to pick up history:

```ts
// TS — Claude Agent SDK: resume / continue / fork
import { query } from "@anthropic-ai/claude-agent-sdk";

// continue: pick up the most recent conversation in this cwd
for await (const m of query({ prompt: "and the population?", options: { continue: true } })) {}

// resume: a specific session id
for await (const m of query({ prompt: "keep going", options: { resume: "9b1c…-uuid" } })) {}

// pin / supply your own id, disable persistence, or plug a custom store
query({ prompt: "…", options: {
  sessionId: "my-own-uuid",     // use a specific UUID instead of auto-generating
  persistSession: false,        // opt out of disk persistence (TS)
  sessionStore: myAdapter,      // custom storage adapter (the extension point)
}});
```

Because the transcript store is a real filesystem object, it is **queryable**:
`listSessions()`, `getSessionMessages(id)`, `getSessionInfo(id)`,
`renameSession(id, name)`, `tagSession(id, tag)`. The session is not an opaque
blob — it is browsable history.

Other ecosystems land in the same place with different names:

- **LangGraph** reframes the session as **graph checkpoints**. A graph has a
  typed `State`; a `Checkpointer` persists state at every superstep, keyed by
  `thread_id`. Built-ins: `MemorySaver`, `SqliteSaver`, `PostgresSaver`.

  ```python
  # Python — LangGraph: thread_id is the session id
  from langgraph.checkpoint.sqlite import SqliteSaver
  graph = builder.compile(checkpointer=SqliteSaver.from_conn_string("threads.db"))
  cfg = {"configurable": {"thread_id": "user-42-thread-1"}}
  graph.invoke({"messages": [("user", "hi")]}, config=cfg)
  graph.invoke({"messages": [("user", "continue")]}, config=cfg)  # resumes the thread
  ```

- **Google ADK** models a `Session` as `(EventLog, State)` — an append-only
  transcript plus a mutable state dict — identified by a
  `(app_name, user_id, session_id)` triple. The storage abstraction is a
  `SessionService`: `InMemorySessionService`, `DatabaseSessionService`
  (SQLAlchemy), `VertexAiSessionService` (managed). (Its `State` scopes blur the
  session/memory line — see §7.)

- **Vercel AI SDK** deliberately has *no* session abstraction:
  `useChat({ id, initialMessages })` keeps `UIMessage[]` in React state, and
  persistence is the application's job — the recommended pattern is to save in
  an `onFinish` callback. "Session = a row in your `chats` table."

### What varies across providers

| SDK | Session type | Default storage | Pluggable backend | Resume | Identifier |
|-----|--------------|-----------------|-------------------|--------|------------|
| OpenAI Agents SDK | `Session` protocol | `SQLiteSession` (mem/file) | implement protocol | by `session_id` | string `session_id` |
| Claude Agent SDK | JSONL transcript file | disk under `~/.claude/projects/…` | `SessionStore` adapter | `resume` / `continue` | session UUID per project |
| Google ADK | `Session` (events + state) | `InMemorySessionService` | `Database…`, `VertexAi…` | by `(app,user,session)` | `(app, user, session)` triple |
| LangGraph | thread via `checkpointer` | `MemorySaver` | implement `Checkpointer` | `thread_id` in config | `thread_id` string |
| Vercel AI SDK | none (client `UIMessage[]`) | none — app wires DB | app concern | rehydrate `initialMessages` | app chooses chat id |
| LlamaIndex | `ChatStore` + memory wrapper | `SimpleChatStore` (JSON) | Redis/PG/Dynamo/Azure | by key | app-chosen key |

The axes of divergence:

- **Storage default** — in-memory (Agents SDK, LangGraph `MemorySaver`,
  ADK) vs. on-disk JSONL (Claude SDK) vs. nothing-at-all (Vercel).
- **Identifier shape** — a single opaque string (`session_id`, `thread_id`)
  vs. a composite tuple (ADK's `(app, user, session)`).
- **What is stored** — a flat message list (Agents SDK), full event lines
  (Claude SDK JSONL), or arbitrary typed graph `State` (LangGraph) which may be
  far more than messages.
- **Queryability** — Claude SDK exposes list/get/tag/rename over the store;
  most others treat it as opaque load/save.

### What's hard

- **The store is a pluggable concern but the *shape* is not standardized.** Each
  SDK invents its own `Session` / `Checkpointer` / `SessionService` / `ChatStore`
  interface. Porting a transcript between them is manual.
- **Locality gotchas.** Claude SDK `continue` resolves "most recent conversation
  *in this cwd*" — leaky if you run from a different directory. Convenience flags
  that depend on ambient filesystem state surprise users.
- **What exactly to persist.** Raw messages? Tool calls and results? Reasoning
  blocks? Partial/streaming states? LlamaIndex's split is instructive: the
  `BaseChatStore` holds the *bytes* (the raw messages) and a separate memory
  class wraps it with a *policy* (window, summary, vector lookup). Bytes vs.
  strategy is a real seam.

---

## 3. ◆ Fork / branch

**Goal.** *"I want to take a conversation at some point in its history and
explore a different continuation — without destroying the original."*

### How it's done today

Branching is "git for conversations." You take a session at message *N* and
start a new line of turns from there; the original past is shared, the future
diverges. Two implementation strategies exist — **copy the prefix** or
**point at the parent** — and the ecosystem has both.

```ts
// TS — Claude Agent SDK: fork copies the prefix into a new id
import { query } from "@anthropic-ai/claude-agent-sdk";

for await (const m of query({
  prompt: "actually, try a more formal tone",
  options: {
    resume: "9b1c…-parent-uuid",
    forkSession: true,           // new session id; transcript begins as a copy of the parent prefix
    resumeSessionAt: "msg-uuid-47",  // (optionally) fork at a specific message, not the tail
  },
})) {}
```

Claude SDK fork is **copy-based**: `forkSession(id, atIndex?)` creates a new id
whose transcript begins as a copy of the parent's prefix up to the fork point,
then diverges. Simple, but it duplicates bytes.

**Pi (earendil-works)** is the purest **pointer-based** model: sessions are
nodes in a DAG with parent ids, and a child session *shares* its prefix with the
parent **without copying** — branching at message 47 just creates a new leaf
pointing at message 47 as its parent. A `SessionManager` owns the tree and can
create a child from any node or navigate ancestors. This is strictly cheaper
than copying and matches how Git stores history.

**LangGraph** branches via **parent checkpoints**. Every checkpoint records a
`parent_config`; to fork, you fetch a prior checkpoint and resume from it,
producing a divergent line that shares the prefix:

```python
# Python — LangGraph: rewind to a prior checkpoint and branch
states = list(graph.get_state_history(cfg))     # newest → oldest checkpoints
fork_point = states[3].config                   # pick a checkpoint to branch from
graph.invoke({"messages": [("user", "what if we tried X instead?")]}, config=fork_point)
# resuming from a non-tail checkpoint creates a fork sharing the prefix up to that point
```

**OpenAI Responses** gets branching for free from server-stored chains (§4):
since each `previous_response_id` names a snapshot, issuing two different
follow-ups from the *same* prior id produces two divergent branches that share
the prefix server-side. You never hold the bytes — you hold ids.

### What varies across providers

- **Copy vs. pointer.** Claude SDK copies the prefix into a new id; Pi and
  LangGraph point at a shared parent; Responses branches by reusing a prior id.
- **First-class vs. emergent.** Pi and Claude SDK expose explicit fork APIs;
  in Responses branching simply *falls out* of how ids chain; in the OpenAI
  Agents `Session` there is no built-in fork (you copy items manually); Vercel =
  "copy a row in your DB."
- **Fork at the tail vs. anywhere.** Some let you branch only from the current
  head; Claude `resumeSessionAt`, Pi's tree, and LangGraph history let you
  branch from any prior point.

### What's hard

- **Cost of copying.** Copy-based fork duplicates the whole prefix; for long or
  binary-heavy transcripts that is expensive. Pointer-based fork avoids it but
  needs a real tree/DAG structure and careful garbage collection of dead leaves.
- **Retrofitting it.** Branching is cheap to design in from day one and painful
  to bolt on later — it forces every store operation to be aware of ancestry,
  not just "the latest array."
- **Branching server-stored state you don't own.** With Responses you can branch,
  but the snapshots live at the provider; you cannot inspect, migrate, or
  back-up the branch points yourself.

---

## 4. ◆ Server-stored chains

**Goal.** *"I want the provider to hold the conversation, so I send only the new
turn and pass a handle to everything that came before."*

### How it's done today

The OpenAI Responses API inverts the baseline. Instead of the client building and
resending the message array, the **server stores each response** (`store: true`,
the default) and the client passes `previous_response_id` — a handle to the prior
turn. The server reconstructs the context from its stored chain; the client sends
only the new `input`.

```python
# Python — OpenAI Responses: previous_response_id chains turns server-side
from openai import OpenAI
client = OpenAI()

r1 = client.responses.create(model="gpt-5", input="What's the capital of France?")
r2 = client.responses.create(
    model="gpt-5",
    input="And its population?",          # only the NEW turn is sent
    previous_response_id=r1.id,            # the server supplies turn 1's context
)
print(r2.output_text)
```

The newer **`conversations` resource** formalizes the chain as a named,
addressable object that multiple responses attach to:

```ts
// TS — OpenAI Responses: a conversations resource as the explicit handle
import OpenAI from "openai";
const client = new OpenAI();

const conv = await client.conversations.create();
const r1 = await client.responses.create({ model: "gpt-5", input: "Hi", conversation: conv.id });
const r2 = await client.responses.create({ model: "gpt-5", input: "Continue", conversation: conv.id });
// state accumulates under conv.id; the client never resends history
```

Why this layer exists, beyond saving upload bytes:

- **The client stops owning history.** With Chat Completions the client rebuilds
  and resends the array every call; with Responses the server reconstructs it
  from the stored chain. The conversation lives at the provider.
- **Caching becomes implicit.** Because the prefix is stable and server-side,
  the provider can prompt-cache aggressively without the client managing cache
  markers (contrast Anthropic's inline `cache_control` and Gemini's explicit
  `cachedContents` resource — see [`05-cross-cutting.md`](05-cross-cutting.md)).
- **Reasoning-token continuity.** This is *the single biggest reason Responses
  exists.* For o-series / reasoning models, Chat Completions drops the reasoning
  tokens after each turn (billed but invisible, and lost as context). With a
  server-stored chain the encrypted `reasoning` items carry forward across
  turns, preserving multi-step reasoning state.
- **Branching falls out.** Each `previous_response_id` names a snapshot; two
  follow-ups from the same id branch the conversation server-side (§3).

### What varies across providers

- **Anthropic Messages has no equivalent.** It is purely client-held: you always
  resend the array.
- **Gemini's `cachedContents` is related but different** — a manual prompt-cache
  primitive (a named KV-state resource with a TTL), *not* a conversation chain.
  It reuses a prefix; it does not store turns or chain responses.
- **Two handle shapes within Responses** — an implicit chain
  (`previous_response_id` pointing turn-to-turn) vs. an explicit
  `conversations` resource that names the whole thread.

### What's hard

- **Layering it under a client-side session double-counts.** The OpenAI Agents
  SDK explicitly warns: if you use both `Session` (client-held items) *and*
  `previous_response_id` (server-held chain) at once, the context gets counted
  twice. Two history layers that both think they own the past must be reconciled.
- **State you don't own.** Retention, deletion, and export are the provider's;
  `store: false` turns it off but then you lose the chain. You cannot migrate a
  Responses chain to another provider — it is not portable.
- **Provider-specific.** Code that relies on server-stored chains does not
  translate to Anthropic or to client-held flows without reintroducing the
  resend-the-array logic.

---

## 5. The three ownership models

Step back and the entire space collapses to a single question: **who holds the
conversation?** Three answers exist, and which one is in play changes how you
*formulate every request*.

| Model | Examples | Where history lives | What the client sends each turn | Resume / durability |
|-------|----------|---------------------|----------------------------------|---------------------|
| **Client-held** | OpenAI Chat Completions · Anthropic Messages · Gemini `generateContent` | In your process / your DB | The **full message array**, rebuilt every call | You own it; durable iff *you* persist it |
| **Server-session** | OpenAI Realtime (WebSocket) | Inside a live connection at the provider | Incremental events (`conversation.item.create`); the socket holds the list | Ephemeral — lost on disconnect unless you mirror events to your own store |
| **Server-stored-by-id** | OpenAI Responses (`previous_response_id`, `conversations`) | At the provider, addressable by id | Only the **new turn** + a **handle** to the prior id | Durable at the provider; you hold ids, not bytes |

What each implies for how you build a request:

- **Client-held** → you must *accumulate and resend* the array; the system prompt
  goes wherever that provider wants it; growth/trimming is your problem; nothing
  survives a restart unless you wrote it down. Maximum control, maximum
  bookkeeping. This is the world §1 and §2 live in.
- **Server-session** → you *stream events into* a connection rather than build a
  request body; the model sees a server-side list you mutate with
  `conversation.item.*` events. Continuity is tied to the socket's lifetime, so
  any app that needs durability must mirror the events to its own store — which
  often *becomes* the persistence/memory layer (see
  [`04-realtime-and-transports.md`](04-realtime-and-transports.md)).
- **Server-stored-by-id** → you *pass a handle, not the data*. The request body
  shrinks to the new turn; reasoning state and caching are handled for you; but
  the conversation is now provider-owned state you can branch but not migrate.

These are independent layers, not mutually exclusive: a runtime can use a
client-held session *and* a server-stored chain at the same time — which is
exactly why the OpenAI Agents SDK has to warn against double-counting (§4). The
choice of ownership model is a property of the *provider and transport*, not of
the conversation itself.

---

## 6. ◆ Memory (cross-conversation, long-term)

**Goal.** *"I want the agent to remember facts about a user across all their
conversations — that they're vegetarian, that their dog is named Bella — and
recall the relevant ones in any future chat."*

### How it's done today

Sessions are verbatim transcripts of *one* conversation. **Memory** is a
*curated, deduplicated* store of facts that persists across *all* of them,
keyed by user/agent rather than by conversation, and retrieved by *relevance*
to the current query rather than by recency. The dominant shape is
**retrieve-before, store-after**: before generating, search memory for facts
relevant to the user's message and inject them into the system prompt; after
generating, extract and store any new facts.

**mem0** is the headline memory layer. It is not a vector DB and not an agent
framework — it sits between an agent and a vector store, with an LLM in front
that *extracts and curates* facts.

```python
# Python — mem0 (OSS, local) and the managed Platform client
from mem0 import Memory          # local: default LLM + vector store (Qdrant)
memory = Memory()
# from mem0 import MemoryClient; memory = MemoryClient(api_key="…")   # managed — near drop-in

# store-after: extract facts from the latest exchange, scoped to a user
memory.add(
    [{"role": "user", "content": "I'm vegetarian and my dog is named Bella"}],
    user_id="alice",
)

# retrieve-before: hybrid search scoped by identifiers
hits = memory.search(query="what should I cook tonight?", filters={"user_id": "alice"}, top_k=3)
# hits => [{"memory": "User is vegetarian", "score": 0.9, ...}, ...]
```

```ts
// TS — mem0
import { Memory } from "mem0ai";
const memory = new Memory();

await memory.add([{ role: "user", content: "I'm allergic to peanuts" }], { userId: "alice" });
const relevant = await memory.search("what should I eat?", { userId: "alice" });
// relevant => [{ memory: "User is allergic to peanuts", score: 0.87, ... }]
```

mem0 scopes memories set-theoretically by identifier, which is its most
distinctive surface:

- `user_id` — facts about an end user, across all their sessions.
- `agent_id` — facts scoped to an agent/persona ("the support agent learned X").
- `run_id` — facts tied to one conversation run.
- `app_id` — app-wide facts.

You can store under `(user_id=alice, agent_id=support)` and recall only that
intersection. The crucial difference from "just use a vector DB" is that `add`
runs an **LLM pipeline**, not a raw upsert:

1. **Extract** — an LLM emits candidate facts as short, declarative, third-person
   statements ("User is vegetarian").
2. **Retrieve neighbors** — each candidate is embedded and used to fetch the
   top-K existing memories in the same scope.
3. **Decide** — a second LLM call classifies each candidate as `ADD` (new),
   `UPDATE` (rewrite an existing memory in place — id preserved, text changes),
   `DELETE` (contradicts and supersedes), or `NONE` (already covered).
4. **Persist** — the decided ops are applied; each row carries `id`, `memory`,
   `metadata`, `created_at`, `updated_at`, and scope ids.

So after 100 conversations mentioning the dog Bella, you have **one** memory
("User's dog is named Bella"), not 100 chunks. mem0 *actively dedupes and
rewrites* — that is the value-add over a raw vector DB.

**Letta** (the productionized descendant of MemGPT) takes the opposite
architectural stance: it is an **agent runtime**, not a plugin. The agent is a
**server-side object** with persistent state in Postgres/SQLite; clients talk to
a Letta server over HTTP. Its contribution is a three-tier, **self-editing**
memory model borrowed from OS memory hierarchies:

- **Core memory** — *always in context*, as labeled blocks (`human`, `persona`).
  Size-limited; under "memory pressure" the agent must compress it.
- **Recall memory** — the full conversation history, searchable via tool calls.
- **Archival memory** — an unstructured, vector-backed long-term store.

The agent has **tools that edit its own memory**: `core_memory_append`,
`core_memory_replace`, `archival_memory_insert`, `archival_memory_search`,
`conversation_search`. The "self-editing loop" is the agent rewriting its own
core blocks more tersely when they fill up.

```python
# Python — Letta: the agent itself is the stateful, server-side object
from letta_client import Letta
client = Letta(base_url="http://localhost:8283")

agent = client.agents.create(
    model="openai/gpt-5",
    embedding="openai/text-embedding-3-small",
    memory_blocks=[
        {"label": "human", "value": "The user's name is Alice."},
        {"label": "persona", "value": "I am a helpful assistant."},
    ],
    tools=["web_search", "run_code"],
)

resp = client.agents.messages.create(
    agent_id=agent.id,
    messages=[{"role": "user", "content": "What's my name?"}],
)
# A week later the same agent_id resumes with full memory state intact.
```

```ts
// TS — Letta client
import { LettaClient } from "@letta-ai/letta-client";
const client = new LettaClient({ baseUrl: "http://localhost:8283" });

const agent = await client.agents.create({
  model: "openai/gpt-5",
  embedding: "openai/text-embedding-3-small",
  memoryBlocks: [{ label: "human", value: "The user's name is Alice." }],
});
await client.agents.messages.create(agent.id, {
  messages: [{ role: "user", content: "Remember I prefer metric units." }],
});
```

**LangGraph** draws the session/memory line as **two orthogonal interfaces**:
the `Checkpointer` is within-thread state (the session, §2); a separate
`BaseStore` is cross-thread state (the memory). The store is key-value with
optional embedding search, partitioned by namespace tuples:

```python
# Python — LangGraph: BaseStore is the cross-thread memory layer
from langgraph.store.memory import InMemoryStore
store = InMemoryStore(index={"embed": embeddings, "dims": 1536})

ns = ("user", "alice", "memories")             # namespace tuple scopes the memory
store.put(ns, "diet", {"text": "vegetarian"})
hits = store.search(ns, query="what to cook", limit=3)   # embedding search within the namespace
```

mem0, Zep, and similar layers plug in *here* — at the `BaseStore` slot — while
the checkpointer keeps handling the transcript.

**The vector-DB-as-memory pattern.** Underneath every memory product is a vector
DB (Pinecone, Weaviate, Chroma, Qdrant, pgvector) exposing only
`upsert(id, vector, metadata)` and `query(vector, top_k, filter)`. A memory
*layer* (mem0, Zep, Letta's archival) adds, on top of that raw store: (a) an LLM
to *decide what to remember*, (b) a dedup/update policy, (c) scoping
(user/agent/session), (d) retrieval policies (hybrid, recency-weighted), and
often (e) a managed deployment. **The vector DB is the disk; the memory layer is
the filesystem.** You *can* build memory on a raw vector DB, but you reinvent
extraction, dedup, scoping, and TTL — which is why most teams reach for
mem0 / Zep / Letta. (**Zep** is mem0's near-peer, emphasizing a temporal
knowledge graph over simple fact extraction.)

**Provider-native memory: Anthropic's memory tool.** The layers above are
framework/vendor products that sit beside any model. Anthropic ships memory as a
*model capability* instead — a client-side tool the model drives directly. You
declare `{"type": "memory_20250818", "name": "memory"}` in the `tools` array; the
model then issues tool calls (`view`, `create`, `str_replace`, `insert`,
`delete`, `rename`) against a `/memories` directory, and *your* code executes
them against whatever backend you choose (files, a DB, object storage). Anthropic
defines the tool interface and injects a system-prompt protocol that tells the
model to read its memory directory before each task and write progress as it
goes; the storage is yours to implement.

```python
# Python — Anthropic memory tool: the model reads/writes a /memories directory
resp = client.messages.create(
    model="claude-sonnet-4-5",
    max_tokens=2048,
    messages=[{"role": "user", "content": "Remember I prefer metric units."}],
    tools=[{"type": "memory_20250818", "name": "memory"}],
)
# resp contains a tool_use for `memory` (e.g. command="create", path="/memories/...");
# your handler executes it and returns a tool_result, same loop as any tool (§02).
```

This lands in a genuinely different spot from mem0/Letta/LangGraph. There is **no
extract→decide→dedup pipeline** and no semantic-search retrieval layer: it is a
file-system the *model* curates with its own judgment about what to write, read,
and overwrite — closer to Letta's self-editing tools than to mem0's implicit
curation, but with the storage backend left entirely to the application and no
managed index. The SDKs ship helper scaffolds for the backend
(`BetaAbstractMemoryTool` in Python, `betaMemoryTool` in TypeScript). It pairs
with the server-side context editing of the Compaction section: clear stale
tool/thinking blocks from the live window while the memory directory persists the
facts worth keeping across sessions.

### What varies across providers

| Aspect | mem0 | Letta |
|---|---|---|
| Layer | Plugin / library (or managed API) | Full agent runtime |
| State ownership | App owns the agent; mem0 owns memories | Letta server owns the *agent* itself |
| Memory editing | LLM-driven implicit dedup/update | Explicit self-editing via tools |
| Conversation history | Expects the app to keep messages elsewhere | Native recall memory |
| Deployment | `Memory()` lib or `MemoryClient()` SaaS | Run a Letta server |

- **Implicit vs. explicit curation.** mem0 decides what to remember *for* you via
  its extract→decide pipeline; Letta gives the *agent* tools to edit its own
  memory; LangGraph's `BaseStore` does neither — it is a raw scoped KV+search and
  the policy is yours.
- **Where the agent's state lives.** mem0: stateless agent + stateful memory.
  Letta: stateful agent on a server. LangGraph: state in the graph checkpoint.
- **Scope model.** mem0 uses named ids (`user_id`/`agent_id`/`run_id`/`app_id`);
  LangGraph uses opaque namespace tuples; ADK uses prefix strings
  (`user:`/`app:`/`temp:`, see §7).
- **Tools vs. middleware integration.** Some teams want the *LLM* to decide when
  to recall (memory exposed as a `search_memory` tool); others want automatic
  retrieve-before / extract-after (memory as middleware around the call). Most
  SDKs provide *no* built-in memory hook — the retrieve/inject/store happens in
  application code (Vercel does it in the route handler's `system` + `onFinish`;
  the Agents SDK and Claude SDK expect you to hand the agent a memory *tool*).

### What's hard

- **Extraction quality.** "What is a fact worth remembering?" is an LLM judgment
  call; over-extraction floods the store with noise, under-extraction forgets.
- **Contradiction handling.** Deciding `UPDATE` vs. `DELETE` vs. `NONE` when a
  new statement collides with an old one is the genuinely hard part — and it is
  the entire reason a memory layer exists rather than a vector DB.
- **No standard.** mem0, Zep, Letta, LangGraph's store, and raw vector DBs all
  expose different APIs and different scope models; swapping vendors is a
  rewrite, and the space is actively competitive (Zep, Cognee, every model
  vendor's own roadmap).
- **Where to inject.** Retrieved memories usually go into the system prompt,
  which interacts with caching (a changing system prefix breaks prefix caches)
  and with context growth (§1).

---

## 7. The session-vs-memory distinction

The two halves of this file are genuinely different abstractions. Naming the
difference precisely is what lets a system give them separate backends and
lifecycles.

| Aspect | **Session** | **Memory** |
|--------|-------------|------------|
| Lifetime | One conversation (hours–days) | Across all conversations (months–years) |
| Granularity | Verbatim turn-by-turn messages | Distilled facts, summaries, entities |
| Identifier | `session_id` / `thread_id` / `conversation_id` | `user_id`, `agent_id` (and sometimes `app_id`) |
| Storage | Append-only log | Mutable, deduplicated facts |
| Retrieval | Linear (last *N* turns) or full replay | Semantic search keyed by the current query |
| Compression | Optional (summarization) | Mandatory — that's the point |
| Mutation | Rarely (only fork/branch) | Frequently (`UPDATE`/`DELETE` on contradiction) |
| Source of truth | The transcript | The fact store; transcripts are mere evidence |

The clearest articulations of the line, each with different names for the same
boundary:

- **LangGraph** — `Checkpointer` (session) vs. `BaseStore` (memory).
- **Google ADK** — session `EventLog` (session) vs. `user:`/`app:`-scoped
  `State` (memory).
- **Letta** — recall memory (session-ish) vs. archival memory (memory).

A useful directionality falls out when the two layers stay decoupled: you can
**rebuild memory from transcripts** (re-run extraction over the logs) but not
transcripts from memory. The transcript is primary; the fact store is derived.

### Where the line blurs

- **ADK `State` scopes.** ADK collapses both sides into *one* API using prefix
  scopes: no-prefix keys are session-scoped (session-y), `user:`-prefixed keys
  persist across a user's sessions (memory-y), `app:` is global, `temp:` is
  discarded at session end. It is the only major SDK that bakes user-level
  long-term state into the session API itself.

  ```python
  # Python — ADK: prefix scopes straddle the session/memory boundary
  state["draft"] = "..."             # session-scoped (this conversation only)
  state["user:preferred_lang"] = "es"   # persists across THIS user's sessions  → memory-y
  state["app:flag"] = True              # global to the app
  state["temp:scratch"] = 42            # discarded at session end
  ```

  (ADK *also* offers a separate `MemoryService` —
  `InMemoryMemoryService`, `VertexAiMemoryBankService` — for unstructured
  semantic memory, so a session can be "ingested" into memory at its end and
  searched by future sessions. It is the most opinionated factoring: distinct
  services for sessions, memory, *and* artifacts.)

- **Letta's core memory** is a third category entirely — "memory" by lifetime
  (it survives across sessions) but "session"-shaped by access (it is *always
  in context*). It behaves like a persistent, self-editing system-prompt
  fragment. This is **working / core memory**: a small mutable scratchpad that
  lives between a system prompt and the long-term fact store.

- **Vercel AI SDK** takes no position at all: apps typically save messages to
  Postgres *and* maintain a separate user-facts table, and the SDK has opinions
  about neither.

So the cleanest mental model is **three** independent things, not two:

1. **Session** — ordered transcript of one conversation; serializable; forkable;
   keyed by a session id; primary operation `append`.
2. **Memory** — a queryable store of facts/summaries keyed by *agent-meaningful*
   identifiers (user, agent, app); primary operations `search` and `upsert`; the
   store actively curates (dedup, contradiction handling).
3. **Working / core memory** *(optional)* — a small mutable block always in
   context, somewhere between a system prompt and memory (Letta's core blocks,
   ADK's `temp:`/session `State`).

They share an agent run but want independent backends and lifecycles.

---

## What varies / what's hard

> **What varies.**
> - **Who owns the conversation** is the master variable: client-held (resend
>   the array), server-session (mutate a live connection), or server-stored-by-id
>   (pass a handle). It dictates how you build every request (§5).
> - **System-prompt placement** — in-array (OpenAI Chat) vs. top-level field
>   (Anthropic `system`, Responses `instructions`, Gemini `systemInstruction`).
> - **Session storage and identifier shape** — in-memory vs. on-disk JSONL vs.
>   nothing; single opaque string vs. composite `(app, user, session)` tuple;
>   opaque blob vs. queryable/taggable store.
> - **Fork strategy** — copy-the-prefix (Claude SDK) vs. point-at-parent
>   (Pi tree, LangGraph parent checkpoints) vs. emergent-from-ids (Responses).
> - **Memory architecture** — stateless-agent-plus-memory-plugin (mem0) vs.
>   stateful-agent-on-a-server (Letta) vs. a raw scoped store you write the
>   policy for (LangGraph `BaseStore`).
> - **Scope model** — named ids (mem0) vs. namespace tuples (LangGraph) vs.
>   prefix strings (ADK).
> - **Curation** — implicit LLM dedup (mem0) vs. explicit self-editing tools
>   (Letta) vs. none (raw vector DB).
>
> **What's hard.**
> - **Unbounded growth** of client-held history — every turn re-sends and re-pays
>   for the whole transcript until you window, summarize, or trim, all lossy.
> - **Reasoning continuity** — dropping reasoning blocks between turns degrades
>   multi-step models; the entire reason server-stored chains exist.
> - **Two history layers double-counting** — combining a client-side session with
>   a server-stored chain re-injects the same context twice.
> - **State you don't own** — server-stored chains and Realtime sessions aren't
>   portable, inspectable, or migratable by you; durability is the provider's.
> - **Fork cost and retrofit** — copy-based fork duplicates prefixes; pointer-based
>   fork needs a real DAG; either way, branching is painful to add after the fact.
> - **Memory extraction and contradiction handling** — deciding *what* to remember
>   and *how* to reconcile a new fact with an old one is the genuinely hard,
>   LLM-judgment part, and there is no cross-vendor standard for any of it.
