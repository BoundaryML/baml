# 07 — Durable workflows & orchestration

*When code defines the graph and the model runs inside the steps.*

> Legend: `★ table-stakes` · `◆ advanced` · `▲ frontier`

There is a spectrum of orchestration. At one end, **the model decides the next
step**: a tool loop runs the model, the model emits a tool call, your code runs
it, the result goes back, and the model decides whether to call again or stop.
The shape of the run is emergent — nobody wrote it down in advance. That world
is the subject of [`02-tools-and-agents.md`](02-tools-and-agents.md).

At the other end, **code defines the graph**: a human author writes an explicit
sequence of steps — sequential, parallel, branching, looping — and the model is
invoked *inside* individual steps, not as the thing that chooses the steps. The
control flow is fixed and inspectable; the nondeterminism is confined to the
parts of a step that call a model.

```
model decides the next step  ←─────────────────────────→  code defines the graph
   the agentic tool loop                                     explicit step graph
   (file 02)                                                 (this file)
   emergent, flexible,                                       deterministic,
   hard to audit / replay                                    auditable, durable
```

This file documents the code-defines-the-graph end: composing a step graph,
suspending and resuming (including human-in-the-loop), durable execution that
survives crashes and long waits, streaming per-step events, and how the two ends
of the spectrum compose — a workflow step can call an agent, and an agent's tool
can be a whole workflow.

Checkpointing and persistence of *conversational* state (sessions, memory) live
in [`03-state-sessions-memory.md`](03-state-sessions-memory.md); the
checkpointer used by graph frameworks for durability is the same machinery seen
from a different angle. The runtimes that package agents for deployment are in
[`06-harnesses.md`](06-harnesses.md).

---

## 1. Framing — model-decides vs code-defines-the-graph

A workflow is the right tool when the *shape* of the work is known ahead of time
and you need properties the open-ended loop cannot easily give you:

| You reach for a workflow when you need… | Why the loop struggles |
|---|---|
| **Determinism** — the same input runs the same path | the loop's path is chosen by the model, run to run |
| **Auditability** — "what step are we on, and why" | the loop's trace is a flat list of tool calls |
| **Long-running** — minutes to days, across restarts | a loop is a function call; it dies with the process |
| **Human approval** — pause, wait for a person, continue | a loop has no natural pause/persist/resume point |
| **Fan-out / fan-in** — run N things, join the results | parallel tool calls fan out, but joins are ad hoc |
| **Retries per step** — this step is flaky, retry just it | the loop retries the whole turn or nothing |

The trade is flexibility for control. The loop adapts to inputs the author never
anticipated; the graph does exactly what was drawn and nothing else. Most real
systems use both: a deterministic outer graph with a model-driven step somewhere
inside it (§6).

A useful mental model: a workflow is a **function that can pause, persist itself,
survive a crash, and resume** — possibly in a different process, possibly days
later. Everything in this file follows from that one capability.

---

## 2. ★ Composing a step graph

**Goal.** *"I want to declare an explicit sequence of typed steps — running
some in order, some in parallel, branching on a condition, looping until done —
with each step's output flowing into the next."*

### How it's done today

A step is a typed unit of work: an input schema, an output schema, and an
`execute` function. A workflow wires steps together with combinators. Three
framework shapes dominate, and they agree on the primitives even where they
disagree on syntax.

**TypeScript — Mastra (`createWorkflow` / `createStep`, chained combinators).**
Each step declares `inputSchema` / `outputSchema` (Zod) and an `execute`; the
workflow is a chain of combinators terminated by `.commit()`.

```typescript
// TS — Mastra: sequential, parallel, branch, loop, map, nested
import { createWorkflow, createStep } from "@mastra/core/workflows";
import { z } from "zod";

const fetchDoc = createStep({
  id: "fetch-doc",
  inputSchema: z.object({ url: z.string() }),
  outputSchema: z.object({ text: z.string() }),
  execute: async ({ inputData }) => ({ text: await download(inputData.url) }),
});

const summarize = createStep({
  id: "summarize",
  inputSchema: z.object({ text: z.string() }),
  outputSchema: z.object({ summary: z.string() }),
  execute: async ({ inputData }) => ({ summary: await llmSummary(inputData.text) }),
});

const classify = createStep({
  id: "classify",
  inputSchema: z.object({ text: z.string() }),
  outputSchema: z.object({ label: z.string() }),
  execute: async ({ inputData }) => ({ label: await llmClassify(inputData.text) }),
});

const workflow = createWorkflow({
  id: "doc-pipeline",
  inputSchema: z.object({ url: z.string() }),
  outputSchema: z.object({ summary: z.string(), label: z.string() }),
})
  .then(fetchDoc)                       // sequential: output flows to next input
  .parallel([summarize, classify])      // fan-out; SYNC BARRIER — waits for both
  // output of .parallel is keyed by step id:
  //   { "summarize": { summary }, "classify": { label } }
  .map(async ({ inputData }) => ({      // reshape the keyed object into one record
    summary: inputData.summarize.summary,
    label: inputData.classify.label,
  }))
  .commit();
```

The other combinators, same chain:

```typescript
// TS — Mastra: conditional branch, loops, map-over-array
wf
  // exactly one branch runs; ALL branch steps must share inputSchema/outputSchema
  .branch([
    [async ({ inputData }) => inputData.label === "legal", legalReview],
    [async ({ inputData }) => inputData.label === "spam", dropIt],
  ])

  // loop a step until / while a predicate over its output holds
  .dountil(refineStep, async ({ inputData }) => inputData.score >= 0.9)
  .dowhile(pollStep, async ({ inputData }) => inputData.status === "pending")

  // iterate a step over an array; concurrency is the fan-out width (default 1).
  // foreach is also a SYNC BARRIER — it waits for every item.
  .foreach(processItem, { concurrency: 5 })

  .commit();
```

Two facts worth holding onto: `.parallel([...])` and `.foreach(...)` are
**synchronization barriers** — the chain does not advance until every branch or
item completes. And the branch combinator requires every candidate step to share
the *same* input and output schema, so that whichever path runs, the next step
sees a uniform shape. Loops have no built-in iteration cap; the idiom is to track
an `iterationCount` in the step's own state and `throw` once it exceeds a
threshold, failing the step (and the workflow) rather than spinning forever.

**Nested workflows as steps.** A committed workflow can be dropped into another
workflow wherever a step is expected — composition all the way up:

```typescript
// TS — Mastra: a whole workflow used as one step
const outer = createWorkflow({ id: "outer", /* …schemas… */ })
  .then(prepare)
  .then(docPipeline)   // docPipeline is itself a createWorkflow(...).commit()
  .then(publish)
  .commit();
```

**Python — pydantic-graph (a typed graph of step nodes).** Instead of a fluent
chain, you declare each node as a class whose `run` returns the *next* node (or
`End`). Routing is just which node you return; parallel branches and joins
(reducers that combine fan-out results) are first-class. The graph is statically
typed over a shared state object.

```python
# Python — pydantic-graph: typed step nodes, conditional routing, a join
from dataclasses import dataclass
from pydantic_graph import BaseNode, End, Graph, GraphRunContext

@dataclass
class State:
    text: str = ""
    summary: str = ""
    label: str = ""

@dataclass
class FetchDoc(BaseNode[State]):
    url: str
    async def run(self, ctx: GraphRunContext[State]) -> "Summarize":
        ctx.state.text = await download(self.url)
        return Summarize()

@dataclass
class Summarize(BaseNode[State]):
    async def run(self, ctx: GraphRunContext[State]) -> "Classify":
        ctx.state.summary = await llm_summary(ctx.state.text)
        return Classify()

@dataclass
class Classify(BaseNode[State]):
    # conditional routing: return one of several next-node types
    async def run(self, ctx: GraphRunContext[State]) -> "LegalReview | End[State]":
        ctx.state.label = await llm_classify(ctx.state.text)
        if ctx.state.label == "legal":
            return LegalReview()
        return End(ctx.state)

graph = Graph(nodes=[FetchDoc, Summarize, Classify, LegalReview])
result = await graph.run(FetchDoc(url="https://…"), state=State())
```

pydantic-graph models **joins and reducers** (a node that waits on several
upstream branches and folds their outputs), **decisions** (conditional routing,
shown above as the union return type), and **parallel execution** of independent
branches — the same primitive set as the Mastra chain, expressed as a graph of
nodes rather than a linear combinator stream.

**Python / TS — LangGraph (`StateGraph`).** The other dominant shape. You build
a graph over a typed `State`: `add_node`, `add_edge`, and `add_conditional_edges`
(a router function picks the next node). State updates are *reducers* applied to
the typed channels, so fan-out branches that write the same key are merged rather
than clobbered.

```python
# Python — LangGraph: StateGraph with a conditional edge
from langgraph.graph import StateGraph, START, END
from typing import TypedDict

class State(TypedDict):
    text: str
    label: str

g = StateGraph(State)
g.add_node("classify", classify_node)
g.add_node("legal_review", legal_node)
g.add_edge(START, "classify")
g.add_conditional_edges(
    "classify",
    lambda s: "legal_review" if s["label"] == "legal" else END,
)
app = g.compile(checkpointer=checkpointer)   # checkpointer → §3, §4
```

**TypeScript — Flue (a workflow is a `run()` export wrapping a harness).** A
third shape sits at a different altitude. Where Mastra and the graph frameworks
make the *step graph* the unit and invoke a model inside a step, Flue makes the
*agent* (file 06) the unit of work and the orchestration plain imperative
TypeScript. A workflow is just a module that exports a `run` function — the
filename names the workflow; inside, you initialize the agent into a harness,
open a session, perform one purpose-specific operation, and return its result.

```typescript
// TS — Flue: a workflow is a run() export; the agent is the step, run() is the graph
import summarizer from "../agents/summarizer";
import type { FlueContext } from "@flue/runtime";   // authoring API (consumer client is @flue/sdk)

export async function run({ init, payload }: FlueContext<{ url: string }>) {
  const harness = await init(summarizer);            // initializer → harness (ctx.init, or init from @flue/runtime)
  const session = await harness.session();           // open a session (file 06)
  const result = await session.prompt(`Summarize: ${payload.url}`);
  return { summary: result.text };                   // bounded operation → returned value
}
```

This is the model-decides-vs-code-defines axis seen from the harness world: the
agent decides *within* a step (the open-ended loop of file 02), and the `run()`
body — any branching, fan-out, or sequencing you write in TypeScript — is the
code-defined part. The framing is an app-shape distinction, not a combinator set:
Flue separates an **agent-only** app (a continuing assistant with identity and
sessions, accepting interactions over time) from an **agent + workflow** app (a
bounded, result-oriented operation that runs once and returns — batch jobs,
report generation, scheduled tasks). The `FlueContext` handed to `run` carries
`id`, `payload`, `env`, `req`, `log`, and `init`. It is a **deployable unit**:
`flue run summarize --target node` invokes it locally or from CI, and on
Cloudflare it runs via a deployed endpoint (a direct prompt to an agent is *not*
a workflow run). The harness, session, and agent details it leans on are covered
in [`06-harnesses.md`](06-harnesses.md) rather than duplicated here.

### What varies

- **Linear chain vs explicit graph vs `run()` body.** Mastra is a fluent chain of
  combinators; pydantic-graph and LangGraph are node-and-edge graphs where routing
  is a returned node-type or a router function. The chain is easier to read top to
  bottom; the graph expresses arbitrary topologies (cycles, diamonds) directly.
  Flue takes a third position: there is no step DSL at all — the workflow body is
  plain imperative TypeScript and the agent itself is the unit of work, so the
  "graph" is whatever control flow you write around a harness call.
- **Where the next-step decision lives.** Mastra puts it in `.branch(...)`
  predicates; pydantic-graph puts it in the *return type* of a node; LangGraph
  puts it in `add_conditional_edges`. Same decision, three locations.
- **How state flows.** Mastra threads each step's output to the next step's
  input (and keys parallel/foreach outputs by step id). pydantic-graph and
  LangGraph thread a *shared* state object, with LangGraph applying reducers to
  merge concurrent writes.
- **Loop semantics.** Mastra has explicit `.dountil` / `.dowhile`; graph
  frameworks loop by routing an edge back to an earlier node (a cycle), bounded
  by a recursion/step limit.

### What's hard

- **Static typing across edges.** Guaranteeing that step B's input schema is
  satisfied by step A's output — across branches, joins, and reshaping `.map`
  steps — is real type-system work. Mastra leans on Zod inference; pydantic-graph
  on the node return-type union; both fight the same battle at the join points.
- **Fan-in is where the complexity hides.** Fan-out is easy (start N things). The
  join — wait for all, decide what "all" means when one fails, merge results
  without clobbering, key the merged object so the next step can read it — is the
  part every framework spends its design budget on.
- **No standard graph format.** A Mastra chain, a pydantic-graph, and a LangGraph
  `StateGraph` are not interchangeable artifacts. There is no portable IR for "a
  step graph," so a workflow is locked to its framework.

---

## 3. ◆ Suspend / resume & human-in-the-loop

**Goal.** *"I want a workflow to pause — wait for a human to approve, or for an
external event to arrive — persist everything it needs, and resume later,
possibly after the process has restarted or days have passed."*

### How it's done today

This is the capability that separates a workflow from a plain function. A
function cannot stop in the middle, write itself to disk, and be called back into
existence an hour later at the exact line it stopped on. A workflow can.

**TypeScript — Mastra (`suspend()` inside a step, `resume()` to continue).** A
step calls `suspend()` to pause. The runtime snapshots the whole execution state
to the configured storage provider; the run is now dormant — no process need stay
alive. Later, `resume()` re-hydrates the snapshot and continues from that step,
passing in `resumeData` (validated by the step's `resumeSchema`).

```typescript
// TS — Mastra: a human-approval step that suspends and resumes
const approval = createStep({
  id: "approval",
  inputSchema: z.object({ draft: z.string() }),
  resumeSchema: z.object({ approved: z.boolean(), note: z.string().optional() }),
  suspendSchema: z.object({ draft: z.string() }),  // context shown to the approver
  outputSchema: z.object({ approved: z.boolean() }),
  execute: async ({ inputData, resumeData, suspend }) => {
    if (!resumeData) {
      // no resume data yet → pause and persist; surfaces { draft } to the UI
      return await suspend({ draft: inputData.draft });
    }
    return { approved: resumeData.approved };
  },
});

// drive the run
const run = await workflow.createRun();
const first = await run.start({ inputData: { draft } });
// first.status === "suspended"; the run is now persisted and the process can exit.

// …minutes or days later, in a different process, after a human clicks "approve":
await run.resume({ step: approval, resumeData: { approved: true } });
// (the `step` argument may be omitted when exactly one step is suspended)
```

Because the snapshot lives in storage, the run survives **process restarts and
reconnections**: a long-lived UI can disconnect and reconnect, and the run state
is recovered from storage (e.g. `getWorkflowRunById` / a state reader) rather
than held in memory. `.start()` runs the workflow and returns the terminal-or-
suspended result; `.stream()` exposes the same run as an *event stream* (§5), so
a UI can watch progress and react to the suspend event live.

**Python — Pydantic AI / pydantic-graph (human-in-the-loop approval).** The
graph runs until a node needs human input; that node either persists the graph
state and exits, or — in Pydantic AI's tool layer — a tool is marked as requiring
approval, the run pauses with a deferred tool call, and your code resumes it once
a human decides. The graph's typed state is what gets serialized between pause
and resume.

```python
# Python — pydantic-graph: pause at an approval node, persist, resume later
@dataclass
class AwaitApproval(BaseNode[State]):
    async def run(self, ctx: GraphRunContext[State]) -> "Publish | End[State]":
        decision = ctx.state.approval        # filled in on resume
        if decision is None:
            # no decision yet — persist state and stop; an external system
            # will re-run the graph from this node once a human responds.
            raise Pause()                    # framework-specific suspend signal
        return Publish() if decision else End(ctx.state)

# resume path: load the persisted state, set the decision, re-run from the node
state = await load_state(run_id)
state.approval = True
await graph.run(AwaitApproval(), state=state)
```

```python
# Python — Pydantic AI: a tool that requires human approval before it runs
from pydantic_ai import Agent
from pydantic_ai.tools import RequiresApproval   # tool gated on human sign-off

agent = Agent("anthropic:claude-sonnet-4-5")

@agent.tool(requires_approval=True)
async def refund(ctx, order_id: str, amount: float) -> str:
    return await issue_refund(order_id, amount)

result = await agent.run("Refund order 1234 for $40")
# run pauses with a deferred (unapproved) tool call; after a human approves,
# resume the run feeding the approval back in — the model never auto-fires it.
```

LangGraph expresses the same idea with an `interrupt()` call inside a node plus a
checkpointer: the graph halts, the checkpoint persists, and `Command(resume=…)`
continues it. The checkpointer is the same persistence mechanism covered in
[`03-state-sessions-memory.md`](03-state-sessions-memory.md).

### What varies

- **What "suspend" looks like in code.** Mastra: call `suspend()` and return.
  Pydantic AI: mark a tool `requires_approval`. LangGraph: call `interrupt()`.
  Three idioms for the same pause-persist-resume.
- **How the pause surfaces to the human.** A `suspendSchema` payload (Mastra), a
  deferred tool call to inspect (Pydantic AI), an interrupt value (LangGraph) —
  the contract for "here is what you're approving" differs per framework.
- **Resume addressing.** By step id + `resumeData` (Mastra), by re-running from
  the persisted node (pydantic-graph), by `Command(resume=…)` against a thread id
  (LangGraph).

### What's hard

- **Serializing live state.** Everything the step needs on resume must be
  serializable. Closures, open sockets, in-flight model streams, and large blobs
  don't snapshot cleanly; the author has to keep step state plain-data.
- **Schema drift across the wait.** A run suspended on Monday may resume on a
  Friday deploy where the step's code or schema changed. Versioning the workflow
  definition so old snapshots still resume is a genuine migration problem.
- **Exactly-once resume.** A human clicking "approve" twice, or two replicas both
  resuming the same run, must not double-execute the continuation — which pushes
  straight into the durability and idempotency concerns of §4.

---

## 4. ◆ Durable execution

**Goal.** *"I want a workflow to survive transient failures, process restarts,
and long waits — so that a crash mid-run resumes from the last completed step
instead of starting over, and a step that already ran is never re-run for real."*

### How it's done today

Suspend/resume (§3) handles *intentional* pauses. Durable execution handles
*unintentional* ones — the machine dies, the deploy rolls, the network blips —
and the long, boring waits in between. The mechanism is the same in spirit:
**checkpoint after every step**. Each completed step's result is persisted; on
restart the engine **replays** the workflow, skipping steps whose results are
already checkpointed and only actually executing the unfinished tail.

Replay implies **at-least-once** execution: a step may be attempted more than
once (crash after the side effect but before the checkpoint). Therefore steps —
especially those with side effects like charging a card or sending an email —
must be **idempotent**, usually via an idempotency key derived from the run id +
step id.

**Python — Pydantic AI durable adapters (Temporal, DBOS, Prefect, Restate).**
Pydantic AI doesn't reimplement durability; it adapts to a durable engine. The
agent/graph becomes a *durable workflow* whose steps the engine checkpoints,
retries, and replays. The model calls become durable *activities*.

```python
# Python — Pydantic AI on Temporal: the agent run is a durable workflow
from pydantic_ai import Agent
from pydantic_ai.durable_exec.temporal import TemporalAgent

agent = Agent("anthropic:claude-sonnet-4-5", tools=[lookup, charge])
durable = TemporalAgent(agent)   # wraps model calls + tools as Temporal activities

# Inside a Temporal workflow function:
async def billing_workflow(req: BillingRequest) -> str:
    # each model call / tool call is checkpointed; a crash resumes from the last
    # completed activity instead of re-running the whole agent from the top.
    result = await durable.run(req.prompt)
    return result.output
```

The same agent can be backed by **DBOS** (durability via a Postgres-backed step
log, no separate server), **Prefect** (task-graph orchestration with retries),
or **Restate** (durable execution + a built-in event/state store) — by swapping
the adapter, not the agent. The engine supplies checkpoint-per-step, automatic
per-step retries with backoff, and restart resilience.

**TypeScript — Mastra (built-in persistence; Inngest as an engine).** Mastra
persists workflow run state to its storage provider, so a run survives restarts
and can be recovered by id (§3). For full durable execution — durable timers,
managed retries, event-driven steps — Mastra integrates an engine such as
**Inngest**, which turns each step into a durably-executed function with
automatic checkpointing and replay.

```typescript
// TS — Mastra workflow executed durably via the Inngest engine
import { init } from "@mastra/inngest";

const { createWorkflow, createStep } = init(inngest);

const charge = createStep({
  id: "charge",
  inputSchema: z.object({ orderId: z.string(), amount: z.number() }),
  outputSchema: z.object({ chargeId: z.string() }),
  execute: async ({ inputData }) => {
    // idempotency key ties the side effect to (run, step) so a replay
    // after a crash does NOT charge the card twice.
    return { chargeId: await stripe.charge(inputData, { idempotencyKey: key }) };
  },
});
// Inngest checkpoints each step; on failure it retries the step, on restart it
// replays completed steps from the log and resumes the unfinished tail.
```

LangGraph reaches durability through its **checkpointer**: every super-step is
saved (in-memory, SQLite, Postgres, …), so a thread can be resumed after a crash
from its last checkpoint — the persistence detail covered in
[`03-state-sessions-memory.md`](03-state-sessions-memory.md).

### What varies

| | Where durability lives | Retry granularity | Long waits |
|---|---|---|---|
| Temporal | external server, event-sourced replay | per-activity, configurable | durable timers (days/months) |
| DBOS | Postgres step log, in-process | per-step | DB-backed sleep |
| Prefect | task runs in a Prefect backend | per-task | scheduled / awaited |
| Restate | durable execution + state store | per-handler | durable promises/timers |
| Inngest | event-driven engine, step memoization | per-step | durable `step.sleep` |
| LangGraph | checkpointer (pluggable store) | per super-step | resume from checkpoint |

### What's hard

- **Idempotency is the author's burden.** At-least-once means the framework
  *will* re-run a step on recovery. Making the *effect* once-only (idempotency
  keys, dedupe tables, conditional writes) is application work the engine cannot
  do for you.
- **The model call inside a durable step is nondeterministic.** Replay-based
  engines assume a step, re-run with the same inputs, produces the same output.
  An LLM call doesn't. The fix is to treat the model call as a checkpointed
  *activity* whose result is recorded and replayed — not re-invoked — on
  recovery. Getting that boundary right (record the response, never re-sample)
  is the central correctness issue.
- **Determinism rules in the workflow body.** Engines forbid wall-clock reads,
  random numbers, and direct I/O in the orchestration code (only inside
  checkpointed steps/activities), because the body is replayed. This constrains
  how the graph is written.
- **State size and retention.** Checkpoints accumulate; large step outputs bloat
  the store. Deciding what to persist vs recompute, and when to garbage-collect
  finished runs, is an operational tax.

A related concern is the *live model window* a long-running workflow carries
across steps. Over many milestones it accumulates tool logs, retries, and stale
reasoning that no longer earn their place in context. **Compaction** is the
per-context tool for keeping that window sharp: the provider returns
machine-state items that *replace* the prior window and are carried forward
verbatim — not a human-readable summary. In a durable workflow you typically
compact after a milestone step (a phase finishes, a root cause narrows) and
persist the compacted state as the checkpoint the next step resumes from, so it
composes directly with the per-step checkpointing/durability above. The full
treatment is in
[`03-state-sessions-memory.md`](03-state-sessions-memory.md) (its
"Compaction" section).

---

## 5. ◆ Streaming & observability of a workflow

**Goal.** *"I want to watch a workflow run step by step — emit progress events
for a UI and traces for monitoring — and to inspect or replay individual steps
after the fact."*

### How it's done today

A workflow is a natural source of structured events: *step started*, *step
finished* (with its output), *workflow suspended*, *workflow completed*. Where a
plain agent loop emits a flat stream of tokens and tool calls, a workflow emits a
**hierarchy** keyed by step id.

**TypeScript — Mastra (`.stream()` event stream).** `.start()` runs to
completion (or suspension) and returns the result; `.stream()` runs the same
workflow but yields events as steps execute — the way a UI subscribes to
progress and notices a suspend event the moment it happens.

```typescript
// TS — Mastra: per-step progress events
const run = await workflow.createRun();
for await (const event of run.stream({ inputData: { url } })) {
  switch (event.type) {
    case "step-start":     console.log("→", event.payload.stepId); break;
    case "step-result":    console.log("✓", event.payload.stepId, event.payload.output); break;
    case "step-suspended": notifyApprover(event.payload); break;
    case "workflow-finish": console.log("done", event.payload.result); break;
  }
}
```

**Python — pydantic-graph / Pydantic AI.** Iterating the graph yields each node
as it runs (`async with graph.iter(...) as run: async for node in run:`), and the
agent layer exposes per-step events and OpenTelemetry spans. Steps become spans
in a trace, so a step graph renders as a span tree in any OTel backend —
per-step latency, inputs, outputs, and errors attributed to the step that
produced them.

```python
# Python — pydantic-graph: observe each node as it executes
async with graph.iter(FetchDoc(url=url), state=State()) as run:
    async for node in run:
        emit_event(type="step", node=type(node).__name__)   # → progress UI / span
```

**Time-travel / replay of a single step.** Because each step's input and output
are checkpointed (§4), frameworks can re-run *one* step in isolation against its
recorded input — to debug a flaky step, or to resume from an earlier checkpoint
after editing state. Mastra resumes from a specific step id with new
`resumeData`; LangGraph rewinds a thread to a prior checkpoint and continues
("time-travel"); pydantic-graph re-runs from a persisted node. The granularity is
the defining win over a flat loop trace: you replay *a step*, not the whole run.

### What varies

- **Event vocabulary.** Step-start / step-result / suspended / finish (Mastra),
  node iteration (pydantic-graph), super-step updates (LangGraph) — no shared
  event schema.
- **Trace shape.** Some frameworks emit OTel spans natively (one span per step);
  others expose only an in-process event stream that you must forward to a
  tracer yourself.
- **Replay granularity.** Per-step (Mastra), per-node (pydantic-graph), per
  super-step / per-checkpoint (LangGraph).

### What's hard

- **Streaming model tokens *through* a workflow.** A step that calls a streaming
  model produces a token stream nested inside the workflow's step-event stream.
  Multiplexing the inner token stream and the outer step stream into one ordered
  feed a UI can render is fiddly.
- **Correlating steps with model spans.** A single step may make several model
  calls and tool calls; attributing cost and latency to the right step (and the
  right model call within it) requires careful span nesting.
- **Replay fidelity.** "Re-run this step" is only safe if the step is pure given
  its recorded input — the same nondeterminism problem as §4, now in service of
  debugging rather than recovery.

---

## 6. ◆ Agents inside workflows, workflows inside agents

**Goal.** *"I want a workflow step to invoke an agent (a model-driven loop), and
I want an agent's tool to be an entire workflow — so the deterministic graph and
the open-ended loop compose in both directions."*

### How it's done today

The two ends of the spectrum nest. A deterministic graph can have one step whose
implementation is an agent that runs its own tool loop (file 02). Conversely, an
agent can be handed a tool that, when called, executes a whole durable workflow —
the model decides *whether* to invoke it, the workflow decides *how* the work is
done. This is the common production shape: a fixed outer skeleton with a
model-driven step inside, or a model that can reach for a durable sub-process.

```typescript
// TS — Mastra: a workflow step whose body is an agent run
const triage = createStep({
  id: "triage",
  inputSchema: z.object({ ticket: z.string() }),
  outputSchema: z.object({ category: z.string() }),
  execute: async ({ inputData }) => {
    const res = await supportAgent.generate(inputData.ticket);  // agent loop inside a step
    return { category: res.object.category };
  },
});

const wf = createWorkflow({ id: "support" })
  .then(triage)                       // model-driven step
  .branch([                           // …inside a deterministic graph
    [async ({ inputData }) => inputData.category === "billing", billingFlow],
    [async ({ inputData }) => inputData.category === "tech", techFlow],
  ])
  .commit();
```

```python
# Python — Pydantic AI: a whole workflow exposed to the model as one tool
agent = Agent("anthropic:claude-sonnet-4-5")

@agent.tool
async def run_onboarding(ctx, customer_id: str) -> str:
    # the model chooses to call this; the body runs a durable, multi-step graph
    result = await onboarding_graph.run(Start(customer_id), state=State())
    return result.output
```

### What varies

- **Direction of nesting offered.** Some frameworks make "workflow as a tool"
  first-class (a workflow is callable wherever a tool is); others only make
  "agent as a step" easy. Both directions are usually achievable, but one is
  often more idiomatic per framework.
- **Whether the inner agent is itself durable.** An agent run inside a durable
  step may or may not checkpoint its individual tool calls — depends on whether
  the agent is wrapped by the durable adapter (§4) or just called normally.

### What's hard

- **Two nondeterminism models meet.** The outer graph wants determinism and
  replay; the inner agent is intrinsically nondeterministic. The boundary — make
  the agent step a single checkpointed activity whose recorded output is replayed
  — is exactly the §4 problem, and it is easy to get subtly wrong when the agent
  loop itself spans multiple model calls.
- **Trace and cost attribution across the boundary.** Tokens spent by an inner
  agent must roll up to the outer step in observability (§5), or per-step cost
  numbers lie.

---

## 7. Survey / contrast

How the prominent orchestration frameworks line up. Control-flow primitives,
suspend/resume, what backs durability, human-in-the-loop, and streaming.

| | Control-flow primitives | Suspend / resume | Durability backing | Human-in-the-loop | Streaming |
|---|---|---|---|---|---|
| **Mastra workflows** | `.then` · `.parallel` (barrier) · `.branch` · `.dountil`/`.dowhile` · `.foreach` (barrier) · `.map` · nested workflows | `suspend()` in step → snapshot → `resume({step, resumeData})` | built-in state persistence; Inngest engine for full durable exec | `suspendSchema`/`resumeSchema` approval steps | `.stream()` per-step event stream |
| **pydantic-graph** | typed step nodes; decisions (routing via return type); joins & reducers; parallel branches | persist typed graph state; re-run from node | adapters: **Temporal, DBOS, Prefect, Restate** | approval nodes / deferred-approval tools | `graph.iter` node-by-node; OTel spans |
| **LangGraph** | `StateGraph` nodes; edges; `add_conditional_edges`; cycles for loops | `interrupt()` + `Command(resume=…)` | pluggable **checkpointer** (file 03) | `interrupt()` for review | super-step event stream; time-travel replay |
| **Flue** | no step DSL — a `run()` export; orchestration is plain TS around a harness call; the agent is the unit of work (file 06) | n/a as a graph primitive — runs once, returns a result; durability lives at the deployment target | a deployable unit invoked by `flue run` / a Cloudflare endpoint; agent-only vs agent+workflow app shapes | handled inside the agent/harness (file 06), not a workflow construct | session/harness stream from the agent (file 06) |
| **Temporal** | code-as-workflow (any control flow in the host language) | first-class (durable timers, signals) | event-sourced replay engine (server) | signals / `await` on a human signal | activity/event history |
| **Inngest** | event/step functions (`step.run`, `step.sleep`, `step.waitForEvent`) | `waitForEvent` / durable sleep | event-driven engine, step memoization | `waitForEvent` on an approval event | step-level run history |
| **Restate** | durable handlers, durable promises | durable promises / awakeables | durable execution + built-in state store | awakeable resolved by a human action | invocation/journal events |
| **DBOS** | decorated steps/workflows in-process | resumable via the step log | Postgres-backed step log (no server) | workflow waits on a recorded event | step log / OTel |

The right edge of the table (Temporal · Inngest · Restate · DBOS) are *general*
durable-execution engines; the left edge (Mastra · pydantic-graph · LangGraph)
are *LLM-oriented* graph frameworks that increasingly **delegate** durability to
the engines on the right (Pydantic AI → Temporal/DBOS/Prefect/Restate; Mastra →
Inngest). The split is real: graph frameworks own the model-call ergonomics and
the step vocabulary; engines own checkpointing, replay, retries, and timers.

Flue is the odd one out — and deliberately so. It declines the step-graph framing
entirely: the orchestration unit is an *agent* (file 06), not a step, and the
"workflow" is just a `run()`-export wrapper that makes that agent a bounded,
deployable, result-returning invocation as opposed to the agent-only continuing
form. It belongs in this file as a contrast point, not a competitor to the graph
columns: there is no combinator vocabulary or resume protocol to compare,
because the control flow is whatever TypeScript you write around the harness.

---

## What varies / what's hard (callout)

Pulling the threads together — the structural difficulties any layer that wants
to offer durable, code-defined orchestration has to absorb:

- **Determinism meets the nondeterministic loop.** The whole value of a graph is
  that it runs the same way twice; the whole nature of an LLM call is that it
  doesn't. Every framework has to draw a line where the nondeterminism is fenced
  into a checkpointed step whose *recorded* output is replayed, never
  re-sampled. Where exactly that line sits — and whether an inner agent loop
  (multiple model calls) fits inside one durable step — is unsettled and easy to
  get wrong.

- **Durability forces idempotency onto the author.** Checkpoint-and-replay gives
  at-least-once execution; a step *will* sometimes run twice. Making the *effect*
  exactly-once (idempotency keys, dedupe, conditional writes) is application
  work no engine can do for you, and it is the most common source of subtle
  production bugs in durable workflows.

- **Suspend/resume is a serialization problem.** Pausing for days and resuming in
  another process means everything the step needs must be plain, serializable
  data — no closures, no live sockets, no in-flight streams. And a run suspended
  before a deploy must still resume after it, which makes workflow-definition
  versioning and snapshot migration a first-class concern.

- **Where the model call sits is the load-bearing design decision.** Inside a
  durable step it must be recorded-and-replayed; streamed through a workflow it
  must be multiplexed into the step-event stream; nested as an agent-in-a-step it
  must roll its tokens and cost up to the enclosing step. The same model call has
  different obligations depending on its position in the graph.

- **No standard across frameworks.** A Mastra chain, a pydantic-graph, a
  LangGraph `StateGraph`, and a Temporal workflow are not interchangeable. There
  is no portable representation of "a durable step graph," no shared event
  vocabulary for step streams, and no common resume protocol — so a workflow,
  and everything observing it, is locked to the framework it was written in.
