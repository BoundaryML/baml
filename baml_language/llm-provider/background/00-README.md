# What people do with LLMs — a landscape

This is a map of **everything people build with LLMs today**, written provider-agnostically.
It exists so a reader can see the whole surface area in one place: the kinds of calls,
loops, state, transports, and runtimes that show up across the ecosystem — and, for each,
how it's done today, what varies between providers, and what's genuinely hard.

It is **landscape, not opinion.** No framework is being pitched; nothing here says what
anyone "should" build. Just what exists.

## The three altitudes

People work at three stacked levels. Most of this collection describes the bottom two; the
top one is its own document because plenty of people consume it directly.

```
┌──────────────────────────────────────────────────────────────────────┐
│  HARNESS / agent runtime                                    → file 06  │
│    a long-lived, controllable, embeddable, deployable agent            │
│    (Claude Code SDK, OpenAI Agents Runner, Pi, Flue, Google ADK)       │
├──────────────────────────────────────────────────────────────────────┤
│  COMPOSITION / loops & workflows                        → files 02,07  │
│    agent loop · handoffs · guardrails · durable step graphs            │
├──────────────────────────────────────────────────────────────────────┤
│  CAPABILITIES / the call                          → files 01,03,04,05  │
│    text · structured output · streaming · multimodal · reasoning ·     │
│    state · realtime · caching · reliability · cost                     │
└──────────────────────────────────────────────────────────────────────┘
```

## The files

| File | What it covers |
|---|---|
| [`01-single-turn.md`](01-single-turn.md) | One answer from a model: text, typed/structured output, streaming, multimodal in/out, reasoning, tokens. |
| [`02-tools-and-agents.md`](02-tools-and-agents.md) | Letting the model act: function calling, the agentic loop, parallel tools, the tool taxonomy, multi-agent, guardrails. |
| [`03-state-sessions-memory.md`](03-state-sessions-memory.md) | Making it remember: history, sessions, fork/branch, server-stored chains, and long-term memory. |
| [`04-realtime-and-transports.md`](04-realtime-and-transports.md) | Live conversations: bidirectional voice, barge-in, and the HTTP→SSE→WebSocket→WebRTC transport taxonomy. |
| [`05-cross-cutting.md`](05-cross-cutting.md) | Production concerns: provider diversity, reliability, caching, observability, cost, deployment. |
| [`06-harnesses.md`](06-harnesses.md) | Using a whole agent runtime: the control plane, permissions/sandbox, extensibility, embedding & deployment. |
| [`07-workflows-and-orchestration.md`](07-workflows-and-orchestration.md) | Durable workflows: deterministic step graphs, branching/parallel/loops, suspend/resume, human-in-the-loop, crash-resilient execution. |

## The whole surface — as "I want to…"

Every surface area, phrased as the thing someone is actually trying to do.

### The call → [`01`](01-single-turn.md)

- I want to **send a prompt and get text back.**
- I want the model to **return a typed/structured value**, not a blob of text.
- I want to **stream tokens** as they're generated.
- I want to **stream a partial object** that fills in as it arrives.
- I want to **send images, audio, video, or PDFs** as input.
- I want the model to **produce images, audio, or a transcription** as output.
- I want to use a **reasoning model** and (sometimes) see or carry forward its thinking.
- I want to **control and account for tokens** — limits, context windows, usage.

### Acting → [`02`](02-tools-and-agents.md)

- I want the model to **call my functions** and use the results.
- I want it to **loop — call tools, observe, call again — until done**, and I want to control when it stops.
- I want it to **call several tools at once.**
- I want to use **provider-hosted tools** (web search, file search, code interpreter).
- I want the model to **operate a computer** (click, type, screenshot).
- I want to plug in **tools from an MCP server.**
- I want to **approve or deny** a tool call before it runs.
- I want the model to **load only the tools it needs** from a large catalog, not every definition every call.
- I want one agent to **hand off to another**, or **delegate to sub-agents.**
- I want to **orchestrate agents** sequentially, in parallel, or in a loop.
- I want to **guard inputs and outputs** and abort on a tripwire.

### Orchestrating → [`07`](07-workflows-and-orchestration.md)

- I want to **define the steps myself** as a graph — sequential, parallel, branching, looping — instead of letting the model decide.
- I want a workflow that can **pause for human approval** (or external input) and **resume later.**
- I want my multi-step pipeline to **survive a crash or restart** and pick up where it left off.
- I want to **run an LLM call or a whole agent inside a workflow step** — and use a workflow as a tool.

### Remembering → [`03`](03-state-sessions-memory.md)

- I want to **keep the conversation going** across turns.
- I want to **persist a session** and resume it later.
- I want to **fork/branch** a conversation from any earlier point.
- I want the **provider to hold the conversation state** so I don't resend history.
- I want to **decide who owns state** — me, a server session, or a server-stored id.
- I want to **compress a long, cluttered context** while preserving the state the model still needs.
- I want **long-term memory** — facts that persist across all conversations.

### Live → [`04`](04-realtime-and-transports.md)

- I want a **real-time voice conversation** with audio in and out.
- I want to **build a voice agent by composing STT + LLM + TTS** and swap any stage's provider (or run it over a phone line).
- I want the system to **detect when the user stops speaking** and respond.
- I want the user to **barge in / interrupt** the model mid-response.
- I want to **change instructions, voice, or tools mid-session.**
- I want **tool calls during a live session.**
- I want to **pick the right transport** (HTTP, SSE, WebSocket, WebRTC) for my latency needs.
- I want to **fire a long-running request and poll for the result** instead of holding a connection open.

### Production → [`05`](05-cross-cutting.md)

- I want to **use many providers** (and gateways/proxies) behind one surface.
- I want **retries, fallbacks, and load-balancing** when a call fails.
- I want **timeouts and graceful rate-limit handling.**
- I want to **cache** an expensive prompt prefix.
- I want **tracing, token/cost accounting, and evals.**
- I want to **budget and attribute cost** across a multi-call pipeline.
- I want to **deploy** server-side, in a browser, at the edge, in CI, or on a schedule.
- I want to **know up front whether a model can do what I'm asking.**

### Runtimes → [`06`](06-harnesses.md)

- I want to **use a whole agent runtime** instead of wiring the loop myself.
- I want to **drive it live** — interrupt, steer, swap models, run in the background.
- I want to **scope its permissions and sandbox** what it can touch.
- I want to **extend it** with skills, MCP servers, sub-agents, and hooks.
- I want **built-in tools and durable on-disk sessions.**
- I want to **embed and deploy it** from my own program.
- I want **one agent interface that can drive any runtime** (Claude Code, Codex, Pi) behind a single abstraction.

## How to read each section

Every capability section follows the same shape:

1. **Goal** — "I want to…"
2. **How it's done today** — Python + TypeScript, across providers where they differ.
3. **What varies** — the divergence between providers.
4. **What's hard** — the part any framework ends up absorbing.

Sections are tagged **★ table-stakes** · **◆ advanced** · **▲ frontier**.
