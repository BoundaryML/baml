# 06 — Using a whole agent runtime (harnesses)

*Long-lived, controllable, embeddable, deployable agents — the top altitude.*

> Legend: `★ table-stakes` · `◆ advanced` · `▲ frontier`

The files below this one build agents from parts. A single call
([`01-single-turn.md`](01-single-turn.md)) gets one answer. A tool loop
([`02-tools-and-agents.md`](02-tools-and-agents.md)) drives the model to act.
State and sessions ([`03-state-sessions-memory.md`](03-state-sessions-memory.md))
make history survive. This file is about the layer where someone has *already
assembled* all of that into a product and hands it to you whole: a **harness**.

You do not write the loop, pick the tools, manage the transcript, or wire
permissions. You import a runtime — the Claude Agent SDK, the OpenAI Agents
`Runner`, Pi, Flue, Google ADK — point it at a task, and drive it from the
outside. The interesting surface is no longer "how do I make a call"; it is "how
do I *steer* a running agent, fence what it's allowed to do, extend it with my
own tools and skills, find its transcript on disk, and deploy it as a unit that
something else can trigger."

Cross-cutting concerns these runtimes inherit — provider diversity, retries,
caching, observability, deployment shapes — live in
[`05-cross-cutting.md`](05-cross-cutting.md).

---

## 1. What a harness is

**Goal.** *"I want the whole agent, not the parts — give me a running thing I
can talk to, interrupt, restrict, extend, and deploy."*

A harness is an **opinionated, long-lived, controllable, embeddable, deployable**
agent runtime. It sits one altitude above the bare agent loop and is defined by
what it *adds* to that loop:

| The loop gives you | The harness adds |
|---|---|
| Call the model, run tools, repeat until done | **Long-livedness** — the run is a thing you hold a handle to, not a function that returns |
| You write the termination predicate | **A control plane** — interrupt, steer mid-flight, swap the model, change permission mode, rewind files, spawn background work |
| You decide which tools to pass | **An opinionated toolset** — Read/Edit/Bash/Grep/Glob ship built in; the agent already knows how to edit code |
| Tools are whatever you hand it | **Permissions & sandboxing** — allow/deny lists, approval callbacks, execution sandboxes |
| You bolt on your own extras | **Extensibility surfaces** — skills, slash commands, sub-agent definitions, hooks, MCP wiring, filesystem resource discovery |
| State is yours to persist | **On-disk sessions** — JSONL transcripts, a working directory as ambient context, queryable session helpers |
| You deploy the loop yourself | **A deployment story** — the agent is a unit with *triggers* (webhook/cron/subprocess) and a serving model |

The key mental shift: a harness is **driven, not called**. With a single call you
pass everything in and get everything out. With a harness you open it, then
*operate* it over time — sending follow-ups, watching an event stream, issuing
side-channel commands. People genuinely embed these whole: the Claude Code agent
loop inside a CI bot, Pi as an RPC subprocess driven from another language, a
Flue agent behind a Cloudflare webhook, the Agents `Runner` inside a service.

The rest of this file walks the six surfaces a harness exposes, then surveys five
concrete runtimes side by side.

---

## 2. ★ Driving a harness

**Goal.** *"I want to start the agent, then talk to it over time — send the next
message, interrupt it mid-thought, swap the model, tighten permissions, undo its
file edits, kick off background work — without restarting."*

Every harness has two entry shapes and (if it is long-lived) a control plane.

### One-shot vs stateful entry points

The **one-shot** entry point takes a prompt and streams back events until the
task is done; each call is a fresh session. The **stateful** entry point opens a
session you hold and feed repeatedly; it keeps history and accepts side-channel
commands. The Claude Agent SDK is the cleanest illustration because it ships
*both* and names them.

```python
# Python — Claude Agent SDK: one-shot query()
from claude_agent_sdk import query, ClaudeAgentOptions, AssistantMessage, TextBlock

async for msg in query(
    prompt="Find and fix the bug in auth.py",
    options=ClaudeAgentOptions(allowed_tools=["Read", "Edit", "Bash"]),
):
    if isinstance(msg, AssistantMessage):
        for block in msg.content:
            if isinstance(block, TextBlock):
                print(block.text)
```

```python
# Python — Claude Agent SDK: stateful ClaudeSDKClient (same session, interrupts)
from claude_agent_sdk import ClaudeSDKClient, ClaudeAgentOptions

async with ClaudeSDKClient(
    options=ClaudeAgentOptions(allowed_tools=["Read", "Edit"])
) as client:
    await client.query("Analyze the auth module")
    async for msg in client.receive_response():
        ...                                  # first turn

    await client.query("Now refactor it to use JWT")   # SAME session
    async for msg in client.receive_response():
        ...                                  # history carried forward
```

The trade-off is explicit in the SDK's own docs:

| Feature | `query()` | `ClaudeSDKClient` |
|---|---|---|
| Session | New each call | Reused |
| Interrupts | Not supported | Supported |
| Continue chat | New session each time | Maintains conversation |
| Use case | One-off tasks | Continuous conversations |

In TypeScript both fold into one `query()` that returns a `Query` — an
`AsyncGenerator` of events *plus* a control surface. The async generator is the
event stream; the methods on the same object are the control plane.

```typescript
// TS — Claude Agent SDK: the Query is generator + control surface
import { query } from "@anthropic-ai/claude-agent-sdk";

const q = query({
  prompt: "Find and fix the bug in auth.ts",
  options: { allowedTools: ["Read", "Edit", "Bash"] },
});

for await (const message of q) {
  console.log(message);          // iterate the event stream
}
```

```typescript
// TS — the control plane (paraphrased Query interface)
interface Query extends AsyncGenerator<SDKMessage, void> {
  interrupt(): Promise<void>;                                  // stop the current turn
  setPermissionMode(mode: PermissionMode): Promise<void>;      // tighten/loosen mid-run
  setModel(model?: string): Promise<void>;                     // hot-swap the model
  rewindFiles(userMessageId: string,                           // undo file edits back to a turn
              options?: { dryRun?: boolean }): Promise<RewindFilesResult>;
  setMcpServers(servers: Record<string, McpServerConfig>): Promise<McpSetServersResult>;
  streamInput(stream: AsyncIterable<SDKUserMessage>): Promise<void>;
  stopTask(taskId: string): Promise<void>;                     // kill a background task
  supportedModels(): Promise<ModelInfo[]>;
  supportedCommands(): Promise<SlashCommand[]>;
  close(): void;
}
```

### The control-plane verbs

Across harnesses the same verbs recur. The names differ; the intent is the same.

- **`interrupt`** — stop whatever turn is in flight. Only meaningful on a
  long-lived session.
- **steer / follow-up** — inject a message. Two flavors, and Pi names them most
  clearly:
  - **`steer()`** — *immediate* mid-stream injection (the model sees it during
    the current turn).
  - **`followUp()`** — *queued* message that runs after the current turn finishes.
- **`setModel`** — swap the model without losing the session.
- **`setPermissionMode`** — move between e.g. `default` / `acceptEdits` /
  `plan` / `bypassPermissions` while running.
- **`rewindFiles`** — restore the working tree to its state at an earlier user
  turn (a checkpoint/undo for the agent's file edits).
- **background tasks** — kick off non-blocking sub-work and later `stopTask` it.

```typescript
// TS — Pi: steer (now) vs followUp (after this turn)
import { createAgentSession } from "@earendil-works/pi";   // illustrative import

const session = await createAgentSession({ cwd: process.cwd() });

session.prompt("Refactor the parser to streaming.");
session.steer("Actually keep the sync API too.");     // injected mid-turn
session.followUp("When done, run the test suite.");   // queued for after
session.subscribe((event) => { /* text_delta, tool lifecycle, turn done */ });
```

```typescript
// TS — Flue: the harness/session split; session.prompt is the stateful entry
const harness = await init({ model: "anthropic/claude-sonnet-4-6" });
const session = await harness.session();
const { data } = await session.prompt("Summarize the open issues.", {
  schema: v.object({ summary: v.string() }),
});
```

### What varies

- **One entry point or two.** Python Claude SDK splits `query()` /
  `ClaudeSDKClient`; TS folds both into one `query()` with a `continue` flag.
  Flue/Pi expose a session object whose `prompt()` is implicitly stateful.
- **Where the control plane lives.** On the returned handle (Claude `Query`,
  Pi `AgentSession`), or *not at all* — the OpenAI Agents `Runner` has no
  interrupt/steer; you cancel by other means and re-`run`.
- **steer semantics.** Immediate (Pi `steer`) vs queued (Pi `followUp`) is a
  real distinction; many runtimes only offer the queued kind, or none.
- **Undo.** `rewindFiles` (Claude) is rare; most runtimes have no notion of
  reverting the agent's side effects.

### What's hard

- **Mid-flight mutation races the loop.** `interrupt`, `setModel`, and `steer`
  arrive while a turn is streaming; the runtime has to land them at a safe
  boundary (between tool calls, between model round-trips) without corrupting the
  in-progress transcript.
- **"Continue" that depends on hidden state.** Folding stateless and stateful
  into one entry point with a `continue: true` flag means session identity
  depends on the working directory — leaky and surprising.
- **Undo is only as good as the checkpoint.** `rewindFiles` can revert files the
  agent wrote, but not side effects that left the box (a pushed commit, a sent
  email, a row inserted in prod).

---

## 3. ◆ Permissions & sandboxing

**Goal.** *"I want to bound what the agent can do — which tools it may use, when
to ask me first, and where its shell commands actually run."*

A harness that can run `Bash` and `Edit` is dangerous by construction.
Permissions and sandboxing are the two fences: *which* tools (permissions) and
*where they execute* (sandbox).

### Allow / deny and permission modes

The coarsest control is an allow-list / deny-list of tool names, plus a
**permission mode** that sets the default disposition.

```typescript
// TS — Claude Agent SDK: allow/deny + mode
const q = query({
  prompt: "Clean up the build scripts",
  options: {
    allowedTools: ["Read", "Edit", "Grep", "Glob"],
    disallowedTools: ["Bash(rm*)"],          // pattern-scoped denial
    permissionMode: "acceptEdits",           // default | acceptEdits | plan | bypassPermissions
  },
});
```

```python
# Python — Claude Agent SDK: same fences on Options
from claude_agent_sdk import ClaudeAgentOptions

options = ClaudeAgentOptions(
    allowed_tools=["Read", "Edit", "Grep"],
    disallowed_tools=["Bash"],
    permission_mode="plan",                  # plan-only: read & propose, don't act
)
```

Modes name common postures: `default` (ask on risky actions), `acceptEdits`
(auto-approve file edits), `plan` (read and propose but never act),
`bypassPermissions` (no prompts — for trusted CI). The mode can be changed at
runtime (`setPermissionMode`, §2).

### Approval callbacks (human-in-the-loop)

Beyond a static list, the host can hand the harness a **callback** consulted per
tool call. This is the programmatic form of "ask the user."

```python
# Python — Claude Agent SDK: a can-use-tool callback
async def can_use_tool(tool_name: str, tool_input: dict, context) -> dict:
    if tool_name == "Bash" and "rm" in tool_input.get("command", ""):
        return {"behavior": "deny", "message": "destructive command blocked"}
    if tool_name == "Edit":
        return {"behavior": "allow", "updatedInput": tool_input}
    return {"behavior": "ask"}   # fall through to interactive prompt

options = ClaudeAgentOptions(can_use_tool=can_use_tool)
```

The Responses-API hosted-MCP path expresses the same idea declaratively with
`require_approval: "never" | "always" | { always: { tool_names: [...] } }` —
the approval gate is data on the tool spec rather than a callback
(see [`02-tools-and-agents.md`](02-tools-and-agents.md)).

### Execution sandboxes

Where does `Bash` actually run? Harnesses span a spectrum:

- **Virtual / in-process shell.** Flue defaults to a virtual sandbox powered by
  `just-bash` — "dramatically faster, cheaper, and more scalable than running a
  full container for every agent." No real OS process.
- **Local machine.** The agent runs commands directly in the host's working
  directory (`sandbox: "local"`). Fast, unfenced — for trusted CI runners.
- **Real container.** A provisioned container (e.g. Daytona, E2B) gives a real,
  disposable OS with true isolation.

```typescript
// TS — Flue: the virtual sandbox is the default — omit `sandbox` to get it
import { init } from "@flue/runtime";

const harness = await init({
  model: "openrouter/moonshotai/kimi-k2.6",
  // no `sandbox`: the default in-memory just-bash environment
});
```

```typescript
// TS — Flue: wrap a custom just-bash factory with bash() at init() time
import { init, bash } from "@flue/runtime";

const harness = await init({
  model: "openrouter/moonshotai/kimi-k2.6",
  sandbox: bash(myBashFactory),   // custom just-bash factory (BashFactory)
});
```

```typescript
// TS — Flue: a real container via the Daytona connector
import { Daytona } from "@daytona/sdk";
import { daytona } from "../connectors/daytona";

const client = new Daytona({ apiKey: env.DAYTONA_API_KEY });
const sandbox = await client.create();
const harness = await init({ sandbox: daytona(sandbox), model: "openai/gpt-5.5" });
```

### What varies

- **Permission granularity.** Whole-tool allow/deny (most) vs pattern-scoped
  (`Bash(rm*)`) vs per-call callback vs declarative approval (`require_approval`).
- **Where the sandbox lives.** In-process virtual shell (Flue default), local
  process, or a remote container — and whether the choice is per-harness
  (Flue `init({ sandbox })`) or fixed by the runtime.
- **Mode vocabulary.** `plan` / `acceptEdits` / `bypassPermissions` are
  Claude-specific names; other runtimes don't model modes at all and rely purely
  on the tool list.

### What's hard

- **Pattern matching on commands is a sieve.** Denying `Bash(rm*)` doesn't stop
  `find . -delete` or a piped `python -c`. Robust command policy is essentially
  un-winnable with string patterns; the real fence is the sandbox boundary.
- **Approval fatigue vs safety.** Ask on everything and the human rubber-stamps;
  ask on nothing and the agent does damage. The interesting policies are
  conditional, which means the callback needs enough context to decide — and that
  context isn't standardized across runtimes.
- **Sandbox parity.** A virtual shell is cheap but doesn't implement every POSIX
  corner; a command that works in a container may behave differently in
  `just-bash`. The agent can't easily tell which environment it's in.

---

## 4. ◆ Extensibility

**Goal.** *"I want to teach the agent new capabilities — my own tools, reusable
skills, slash commands, specialized sub-agents, lifecycle hooks — and have it
discover project conventions automatically."*

A harness is opinionated but not closed. Five extension surfaces recur.

### Custom tools and MCP wiring

In-process custom tools and external MCP servers sit behind one map. In the
Claude SDK a tool is a four-tuple `(name, description, input_schema, handler)`
bundled into an in-process MCP server.

```typescript
// TS — Claude Agent SDK: an in-process tool, exposed as an MCP server
import { tool, createSdkMcpServer, query } from "@anthropic-ai/claude-agent-sdk";
import { z } from "zod";

const getTemperature = tool(
  "get_temperature",
  "Get the current temperature at a location",
  { latitude: z.number(), longitude: z.number() },     // Zod → inferred args
  async (args) => {
    const r = await fetch(`https://api.open-meteo.com/v1/forecast?latitude=${args.latitude}&longitude=${args.longitude}&current=temperature_2m`);
    const data: any = await r.json();
    return { content: [{ type: "text", text: `Temp: ${data.current.temperature_2m}` }] };
  },
  { annotations: { readOnlyHint: true } }
);

const weather = createSdkMcpServer({ name: "weather", version: "1.0.0", tools: [getTemperature] });
// the model sees the tool as: mcp__weather__get_temperature
```

```python
# Python — Claude Agent SDK: same tool, dict-shaped schema, untyped handler
from claude_agent_sdk import tool

@tool("greet", "Greet a user", {"name": str})       # every key required
async def greet(args: dict) -> dict:
    return {"content": [{"type": "text", "text": f"Hello, {args['name']}!"}]}
```

External MCP servers join the same `mcpServers` map across transports —
`McpStdioServerConfig`, `McpSSEServerConfig`, `McpHttpServerConfig`,
`McpSdkServerConfigWithInstance` (in-process), and a Claude.ai proxy variant. So
"my function" and "a remote MCP server over HTTP" are configured uniformly.

```typescript
// TS — Flue: connect a remote MCP server (streamable HTTP by default)
import { connectMcpServer } from "@flue/runtime";

const github = await connectMcpServer("github", {
  url: "https://mcp.github.com/mcp",
  headers: { Authorization: `Bearer ${env.GITHUB_TOKEN}` },
});
const harness = await init({ model: "anthropic/claude-sonnet-4-6", tools: github.tools });
// ... finally { await github.close(); }
```

### Wiring in peer agents (A2A)

MCP wires the agent to **tools and resources** — it is the agent-to-tool seam.
There is an orthogonal seam: connecting the agent to **other agents** it doesn't
own, run by someone else, possibly built on a different framework. The protocol
that has consolidated this peer axis is **A2A (Agent2Agent)**. Where MCP lets an
agent call a tool, A2A lets an agent **discover and delegate to a peer agent** as
an opaque collaborator — it exchanges tasks and results without exposing the
peer's internal tools, prompts, or state.

A2A is the agent-to-**peer** analog to MCP's agent-to-**tool**. The two are
complementary rather than competing: a single agent commonly speaks MCP downward
(to its tools) and A2A sideways (to other agents). ADK already lists A2A as an
extensibility surface alongside its MCP toolsets (§8) for exactly this reason.

At this doc's altitude the mechanics are:

- **Agent Cards for discovery.** Each A2A agent publishes a JSON **Agent Card** —
  a metadata document describing its skills, endpoint, and supported interaction
  modes — at a well-known URL (`/.well-known/agent-card.json`). A peer fetches the
  card to learn what an agent can do before contacting it, the same role MCP's
  capability list plays for tools.
- **Transport: JSON-RPC 2.0 over HTTP(S).** Calls are JSON-RPC requests; the
  protocol layers **Server-Sent Events** for streaming task updates and
  **asynchronous push notifications** for long-running work — the same
  HTTP → SSE progression the transport taxonomy describes
  ([`04-realtime-and-transports.md`](04-realtime-and-transports.md)).

Governance and status (2026): Google created A2A in April 2025, then **donated it
to the Linux Foundation** (announced June 2025 at Open Source Summit North
America), where it is an Apache-2.0 project under neutral governance with 100+
contributing organizations. The spec is at **v1.x** (v1.0.1 as of May 2026); its
headline v1.0 addition is **Signed Agent Cards** — a cryptographic signature on
the card so a receiving agent can verify it was issued by the claimed domain
owner. IBM's separately developed **ACP (Agent Communication Protocol)**
**merged into A2A** under the Linux Foundation in 2025 rather than competing as a
rival. (Cisco's **AGNTCY** initiative and its OASF signed-agent-card schema sit in
the same discovery/identity layer; the agent-protocol landscape is still
consolidating around the A2A-for-peers / MCP-for-tools split.)

### Skills and slash commands

A **skill** is a packaged, reusable procedure the agent can invoke by name; a
**slash command** is a user-facing shortcut. Flue invokes a skill directly and
types its result; the Claude SDK enumerates available slash commands via the
control plane (`supportedCommands()`).

```typescript
// TS — Flue: invoke a named skill with typed output
const { data } = await session.skill("triage", {
  args: { issueNumber: payload.issueNumber },
  schema: v.object({
    severity: v.picklist(["low", "medium", "high", "critical"]),
    reproducible: v.boolean(),
    summary: v.string(),
  }),
});
```

### Sub-agent definitions

A harness can carry a registry of named sub-agents, each with its own prompt,
restricted tool set, and model. In the Claude SDK they are `AgentDefinition`s,
invoked indirectly through a built-in `Agent` tool.

```typescript
// TS — Claude Agent SDK: sub-agents as named definitions
type AgentDefinition = {
  description: string;       // the parent reads this to decide when to delegate
  prompt: string;            // the sub-agent's system prompt
  tools?: string[];          // restrict its tool set
  model?: string;
  skills?: string[];
  maxTurns?: number;
  background?: boolean;      // fire-and-forget
  permissionMode?: PermissionMode;
};

const q = query({
  prompt: "Review the auth changes",
  options: {
    allowedTools: ["Read", "Grep", "Glob", "Agent"],
    agents: {
      "code-reviewer": {
        description: "Expert reviewer. Use for quality and security reviews.",
        prompt: "You are a code review specialist...",
        tools: ["Read", "Grep", "Glob"],
        model: "sonnet",
      },
    },
  },
});
```

Composition rules worth noting: sub-agents start with a **fresh** conversation
(the parent passes one prompt string), the parent receives only the sub-agent's
**final** message, sub-agents **cannot spawn their own sub-agents**, and
sub-agent messages carry a `parent_tool_use_id`. Flue models the same idea as a
`session.task()` (see §6); the OpenAI Agents SDK and ADK model it as
handoffs / `sub_agents` (see [`02-tools-and-agents.md`](02-tools-and-agents.md)).

### Hooks

Hooks fire on lifecycle events (before/after a tool call, on stop, on session
start) and can observe, mutate, or block. They surface in the Claude SDK event
stream as `SDKHookStartedMessage` / `SDKHookProgressMessage` /
`SDKHookResponseMessage`. They let a host enforce policy (e.g. lint after every
edit, block a tool, inject context) without forking the agent.

### Filesystem resource discovery

Harnesses auto-discover conventions from well-known directories — extensions,
skills, prompts, context files — walking from the project root up through
ancestors and into a per-user global dir.

| Runtime | Project dir | Global dir | Discovers |
|---|---|---|---|
| Claude Code | `.claude/` | `~/.claude/` | agents, commands, skills, settings, hooks |
| Pi | `.pi/extensions/`, `.pi/skills/`, `.pi/prompts/` | `~/.pi/agent/` | extensions, skills, prompts, context files |
| Flue | `.flue/agents/`, `.flue/connectors/` | — | agents, connectors |

Pi's `DefaultResourceLoader` does exactly this and lets a custom loader override
extensions, skills, prompts, context files, and the system prompt. Flue's
**connectors** are an unusual twist: they are *not* npm packages but markdown
installation instructions an AI coding agent reads to write a small TypeScript
adapter into `.flue/connectors/<name>.ts`.

### What varies

- **Tool authoring.** In-process MCP four-tuple (Claude), `defineTool()` with
  TypeBox (Pi), bare functions wrapped automatically (Agents SDK, ADK), Valibot
  schemas (Flue). Nobody agrees on the schema library.
- **MCP transport coverage.** Claude SDK covers stdio/SSE/HTTP/in-process/proxy;
  Flue's first version is HTTP-or-SSE only (no local stdio spawn, no OAuth
  callback, no auto-detect).
- **Skill / command model.** First-class typed `session.skill()` (Flue) vs
  filesystem-discovered slash commands enumerated over the control plane (Claude)
  vs none.
- **Sub-agent dispatch.** A built-in tool (Claude), a list field (Agents SDK,
  ADK), or a dedicated primitive (`session.task()` in Flue).
- **In-process sub-agents vs cross-process peers.** Sub-agents above are agents
  the harness *owns* and spawns; A2A reaches agents it *doesn't* own over the
  wire. ADK exposes both (`sub_agents` and A2A); most runtimes here only model the
  owned kind.

### What's hard

- **Tool *output* typing is unsolved.** Input args infer cleanly from
  Zod/TypeBox/Valibot; outputs are typed loosely (`CallToolResult`, `dict`,
  `unknown`) across every runtime here.
- **Discovery precedence is subtle.** Project vs ancestor vs global, and what
  overrides what, is a per-runtime convention a host has to learn — and a
  misplaced `.claude/` or `.pi/` file silently changes behavior.
- **Wire-name leakage.** In-process tools surface to the model as
  `mcp__{server}__{tool}`; the indirection is invisible until a prompt or a deny
  rule has to name the mangled form.

---

## 5. ◆ Built-in tools + on-disk sessions

**Goal.** *"I want the agent to already know how to read, edit, and run code, and
I want its conversation to live somewhere I can inspect, resume, tag, and
query."*

### The opinionated toolset

The defining feature of a *coding* harness is that it ships with file-and-shell
tools and a system prompt that knows how to use them. The common set:
`Read`, `Edit`, `Write`, `Bash`, `Grep`, `Glob`/`Find`, `Ls`. Pi names them
`readTool`, `bashTool`, `editTool`, `writeTool`, `grepTool`, `findTool`,
`lsTool`. You don't define these — you allow or deny them (§3).

```typescript
// TS — Pi: built-in tools, with factories that bind a custom cwd
import { createReadTool, bashTool, grepTool } from "@earendil-works/pi";  // illustrative

const tools = [createReadTool("/workspace/project"), bashTool, grepTool];
```

### Sessions as JSONL on disk

A harness's session is typically a **persisted transcript on disk**, not an
in-memory object. The Claude SDK writes JSONL under
`~/.claude/projects/<encoded-cwd>/*.jsonl`, where `<encoded-cwd>` is the absolute
working directory with every non-alphanumeric character replaced by `-`. The
**working directory is ambient context**: which `cwd` you run in determines which
sessions you can find and resume.

Three ways to pick up history:

```typescript
// TS — Claude Agent SDK: continue / resume / fork (paraphrased Options)
options: {
  continue: true;            // most recent conversation in this cwd
  resume: "session-uuid";    // a specific session id
  forkSession: true;         // copy history into a NEW id and diverge
  resumeSessionAt: "message-uuid";   // resume at a specific message
  sessionId: "fixed-uuid";   // use a chosen id instead of auto-generating
  sessionStore: myStore;     // custom storage adapter
}
```

Pi models the same idea as a **session tree**: branching creates a new leaf
pointing at the branch-point as parent, rather than copying the prefix. So "fork
at message 47" is a pointer, not a duplicate — closer to how Git stores history
than to Claude's fork-as-copy. The broader session-vs-memory distinction lives in
[`03-state-sessions-memory.md`](03-state-sessions-memory.md).

### Queryable session helpers

Because the transcript is a real store, the SDK exposes helpers to *operate* on
it without resuming the agent:

```typescript
// TS — Claude Agent SDK: the transcript store is queryable
import {
  listSessions, getSessionMessages, getSessionInfo,
  renameSession, tagSession,
} from "@anthropic-ai/claude-agent-sdk";

const sessions = await listSessions();
const msgs = await getSessionMessages(sessions[0].id);
await tagSession(sessions[0].id, "incident-4821");
await renameSession(sessions[0].id, "auth refactor");
```

The OpenAI Agents SDK takes the orthogonal view: a `Session` is a small
protocol (`get_items` / `add_items` / `pop_item` / `clear_session`) with
swappable backends (`SQLiteSession`, in-memory). ADK reconstructs the shown
history from an append-only `EventLog` plus a scoped `State` dict, behind a
`SessionService` (in-memory / database / Vertex). The transcript-as-file model
(Claude, Pi) is more debuggable and portable; the protocol model (Agents, ADK)
is more swappable.

### What varies

- **Storage substrate.** JSONL files keyed by `cwd` (Claude), session tree
  (Pi), `Session` protocol over SQLite/memory (Agents), `EventLog` +
  `SessionService` (ADK), Durable Objects (Flue on Cloudflare).
- **Fork cost.** Copy-the-prefix (Claude) vs pointer-to-parent (Pi tree).
- **Queryability.** First-class `listSessions/tagSession/renameSession`
  (Claude) vs minimal CRUD protocol (Agents) vs none exposed.
- **`cwd` coupling.** Claude sessions are findable only from the same working
  directory; other runtimes key on an explicit id.

### What's hard

- **`cwd`-keyed storage is a footgun.** Resume only works from the same working
  directory; move the project and the transcripts orphan. A `SessionStore`
  adapter is the escape hatch but has to exist from day one.
- **Content blocks buried in nested types.** The Claude
  `SDKAssistantMessage.message` is an Anthropic `BetaMessage`, so the
  interesting content blocks (text, tool_use, thinking) live one level down
  rather than flat in the event union — consumers must reach in.
- **Transcript growth.** A long-lived on-disk session grows unbounded;
  compaction (the SDK emits a `compact_boundary` system message) is itself a
  lossy operation that rewrites history.

---

## 6. ◆ Embedding & deployment

**Goal.** *"I want my host program to drive the agent — over a pipe, an RPC
channel, an async generator, or a webhook — and I want to ship the agent as a
deployable unit that something can trigger."*

There are two questions: how the host *talks to* the harness in-process, and how
the harness is *deployed and triggered* in production.

### How the host drives the harness

- **Subprocess + JSONL over stdio.** The Claude SDK spawns a bundled `claude`
  binary and exchanges newline-delimited JSON messages with it over stdio. The
  language SDK is a thin wrapper; the agent loop runs in the subprocess. A
  `Transport` parameter lets you replace *how the subprocess is spawned* (not the
  LLM backend).
- **JSON-RPC subprocess.** Pi's `runRpcMode` (or `pi --mode rpc --no-session`)
  exposes the agent over JSON-RPC so *any* host language can drive it. Pi also
  ships `InteractiveMode` (full TUI) and `runPrintMode` (single-shot stdin →
  stdout).
- **Async generator in-process.** The Agents `Runner` and the Claude TS `query()`
  run in-process and stream events you iterate directly.

```python
# Python — OpenAI Agents Runner: in-process, synchronous and streamed
from agents import Agent, Runner

agent = Agent(name="Assistant", instructions="Be concise.", model="gpt-4o-mini")

result = Runner.run_sync(agent, "Summarize this PR.")     # blocking
print(result.final_output)

streamed = Runner.run_streamed(agent, "Now explain the risk.")
async for ev in streamed.stream_events():                 # event stream
    ...
```

```bash
# Shell — Pi: drive the agent as a JSON-RPC subprocess from any host language
pi --mode rpc --no-session
# host writes JSON-RPC requests to stdin, reads responses/events from stdout
```

### The agent as a deployable unit with triggers

The harness's deployment story is what most distinguishes it from a bare loop.
Flue is the clearest: an **agent is a source file** under `.flue/agents/<name>.ts`.
At its simplest that file just *default-exports a config factory* — the most basic
Flue primitive. The default export registers an addressable agent whose **filename
is its id**; the factory returns the model defaults and instructions.

```ts
// TS — Flue: an agent is a config factory, default-exported from agents/<id>.ts
import { createAgent } from "@flue/runtime";

export default createAgent(() => ({
  model: "anthropic/claude-sonnet-4-6",
  instructions: "Tell a funny \"hello world\" engineering joke.",
}));
```

The same initializer is what `init(agent)` turns into a **harness** — the handle
for model defaults, tools, sandbox, filesystem, and sessions — and a session
opened from that harness carries the stateful `prompt()` shown in §2.

The deployable form replaces the bare factory with a function: it *declares its
triggers* and exports a handler. The framework is runtime-agnostic — "write once,
deploy anywhere (Node.js, Cloudflare, GitHub Actions, GitLab CI/CD)."

```ts
// TS — Flue: a deployable agent declares its trigger; the URL <id> is the instance
import type { FlueContext } from "@flue/runtime";
import * as v from "valibot";

export const triggers = { webhook: true };           // also: cron, etc.

export default async function ({ init, payload }: FlueContext) {
  const harness = await init({ model: "anthropic/claude-sonnet-4-6" });
  const session = await harness.session();
  const { data } = await session.prompt(
    `Translate to ${payload.language}: "${payload.text}"`,
    { schema: v.object({ translation: v.string() }) },
  );
  return data;
}
// invoked at:  POST /agents/<agent-name>/<id>
// reuse the same <id> to continue the same durable agent instance
```

Flue's three nouns formalize the altitudes of this whole file:

- **Agent** — the source file + its triggers; the `<id>` segment is the durable
  *instance* (one customer / repo / conversation space).
- **Harness** — created by `init()`: model defaults, tools, sandbox, filesystem,
  sessions. (`init({ name })` for isolated harness scopes.)
- **Session** — opened by `harness.session()`: persisted history + metadata. On
  Cloudflare, backed by a **Durable Object**; on Node, in-memory by default.
- **Task** — `session.task()`: one-shot focused child work with its own `cwd`
  and `role`, separate from the session transcript (the sub-agent primitive).

```ts
// TS — Flue: task() is the sub-agent primitive; result feeds the next prompt
const research = await session.task("Research the auth flow and summarize key files.", {
  cwd: "/workspace/project",
  role: "researcher",
});
const answer = await session.prompt(
  `Use this research to draft the plan:\n\n${research.text}`,
);
```

Provider configuration in this world is **runtime config**, not source. Flue's
`registerProvider(name, settings)` sets per-provider endpoint/headers/credentials
once, applying to every harness and session that resolves through that provider —
the closest analog to a declarative client block.

```ts
// TS — Flue: registerProvider in app.ts applies to every harness/session
import { registerProvider } from "@flue/runtime";
import { flue } from "@flue/runtime/routing";
import { Hono } from "hono";

registerProvider("anthropic", {
  baseUrl: env.ANTHROPIC_BASE_URL,
  apiKey: env.GATEWAY_KEY,
});

const app = new Hono();
app.route("/", flue());          // mount Flue's routes on the app
export default app;
```

### Consuming a deployment from another app

Authoring and *consuming* are two distinct surfaces. Everything above is
authoring — you build the agent with `@flue/runtime`. Once it is deployed, an
**external application** calls it over HTTP through a separate client package,
`@flue/sdk`. The client never imports the runtime; it just addresses a running
deployment by URL and token.

```ts
// TS — Flue: consume a DEPLOYED agent/workflow from another app (@flue/sdk, not @flue/runtime)
import { createFlueClient } from "@flue/sdk";

const client = createFlueClient({
  baseUrl: "https://example.com/api",
  token: process.env.FLUE_TOKEN,
});
// client.agents   — invoke an agent instance and stream its events
// client.workflows — start a run
// client.runs      — inspect / stream runs
```

So the split is explicit: **author with `@flue/runtime`** (`createAgent`, `init`,
`session.prompt`, `connectMcpServer`, `registerProvider`); **consume a deployment
with `@flue/sdk`** (`createFlueClient`).

ADK's deployment targets round out the picture: **Vertex AI Agent Engine**
(`agent_engines.create(...)` — managed infra, persistent sessions, tracing),
**Cloud Run** (`adk deploy cloud_run` generates a FastAPI container), and
self-hosted GKE running the same FastAPI server.

### What varies

- **Drive channel.** Subprocess+JSONL (Claude), JSON-RPC subprocess (Pi),
  in-process async generator (Agents, Claude TS), Durable Object / fetch handler
  (Flue), FastAPI server (ADK).
- **Trigger model.** Declarative `export const triggers` (Flue: webhook/cron) vs
  "you wire the server" (Agents, ADK Cloud Run) vs "you spawn the process"
  (Claude, Pi).
- **Provider selection.** Env-var dispatch inside the bundled binary
  (`CLAUDE_CODE_USE_BEDROCK=1`), prefix-routed model strings
  (`anthropic/claude-sonnet-4-6`, Flue/OpenRouter), or `registerProvider`
  runtime config. See [`05-cross-cutting.md`](05-cross-cutting.md).
- **Instance identity.** A URL `<id>` segment (Flue), a session id (Claude/Pi),
  a `session_id` + `user_id` (ADK), an in-memory object (Agents default).

### What's hard

- **Subprocess lifecycle.** A spawned `claude`/`pi` binary is a child process to
  supervise — crashes, zombie cleanup, stdio backpressure, version skew between
  the wrapper and the bundled binary.
- **Durable instance identity.** "Same `<id>` continues the same agent" means the
  deployment substrate (Durable Object, DB session) must guarantee
  single-writer semantics under concurrent triggers.
- **Triggers are not in the SDK.** Webhook/cron framing is a deployment-target
  concern; only Flue lifts it into the agent declaration. Everywhere else the
  host re-invents serving, scheduling, and idempotency.

---

## 7. ▲ Wrapping harnesses behind one abstraction

**Goal.** *"I don't want to bind my code to one runtime's bespoke SDK. I want a
single agent interface that can drive Claude Code, Codex, or Pi — and whose output
flows into the same streaming/UI surfaces I already use for plain model calls."*

This is the newest move in the space: the same abstraction step that was applied to
*providers* (one `LanguageModel` interface over many APIs — see
[`05-cross-cutting.md`](05-cross-cutting.md)) being applied to *harnesses* — one
agent interface over many runtimes. The Vercel AI SDK v7 makes it explicit with a
`HarnessAgent` and per-runtime **adapter packages** (e.g.
`@ai-sdk/harness-claude-code`). It draws the line this whole file is built on, in so
many words: *providers expose models to `generateText`/`streamText`; harnesses expose
agent runtimes to `HarnessAgent`* — decoupled abstractions that share compatible
primitives.

A harness here is defined as *"a complete agent runtime that owns capabilities larger
than a model call: workspace access, built-in coding tools, native session state,
compaction, permission flows, and runtime-specific configuration."* The abstraction
keeps a runtime's native behavior intact while exposing a uniform handle.

```ts
// TS — Vercel AI SDK v7: one HarnessAgent interface, a Claude Code runtime underneath
import { HarnessAgent } from "ai";
import { claudeCode } from "@ai-sdk/harness-claude-code";

const agent = new HarnessAgent({ harness: claudeCode({ /* sandbox, model, … */ }) });

const session = await agent.createSession();          // owns runtime + sandbox + history
try {
  const result = await agent.generate({               // → AI SDK GenerateTextResult
    session,
    prompt: "Inspect the repository and summarize the test setup.",
  });
  console.log(result.text);
} finally {
  await session.destroy();
}
```

The session is the live conversation-and-workspace object. It carries *"the harness
runtime, sandbox, working directory, native conversation history, and pending
approvals."* For server routes it can be parked and resumed rather than torn down:

```ts
// TS — stream, and hand session state across a server boundary
const stream = await agent.stream({ session, prompt });   // → AI SDK StreamTextResult
// stream.toUIMessageStream() feeds the same useChat() UI as a plain model call

await session.detach();   // persist/relinquish without destroying (server routes)
await session.stop();     // halt an in-flight turn
```

Notable: harness output is translated into the SDK's ordinary stream types, so a
runtime as heavy as Claude Code plugs into the *same* `useChat` / `toUIMessageStream`
surfaces as a one-shot `streamText` call. Custom instructions, **skills**, and
ordinary AI SDK **tools** layer on top of the runtime's native behavior.

### What varies

- **Altitude of the abstraction.** This is a layer *over* the runtimes surveyed below
  — analogous to LiteLLM / the Vercel provider registry sitting over raw provider
  APIs. The runtime (Claude Code, Codex, Pi) still exists underneath; the adapter
  normalizes it.
- **What's normalized vs. preserved.** Sessions, sandbox, permissions, and streaming
  are surfaced uniformly; runtime-specific configuration and native behavior are
  explicitly *passed through*, not erased.
- **Maturity.** This is a v7, actively-moving surface — names and shapes
  (`HarnessAgent`, adapter package layout, `detach`/`stop` semantics) may still shift.

### What's hard

- **Lossy normalization.** Every runtime has capabilities the common interface can't
  fully express (Claude Code's `rewindFiles`, Pi's `steer`); a uniform `generate`/
  `stream` either drops them or routes them through an escape hatch — the same
  best-effort-vs-feature-parity tension providers hit (see
  [`05-cross-cutting.md`](05-cross-cutting.md)).
- **Session lifecycle across server boundaries.** `detach`/`stop`/`destroy` exist
  because a harness session owns a sandbox and a child runtime that must be parked or
  reclaimed correctly across stateless request handlers — the durable-instance
  problem from §6, surfaced as explicit API.
- **Two abstractions to keep compatible.** Providers and harnesses are decoupled but
  meant to share stream/tool/UI primitives; keeping a `HarnessAgent` stream
  indistinguishable from a `streamText` stream at the UI is real ongoing work.

---

## 8. Survey table

What each runtime actually gives you, across the surfaces above:

| | **Claude Agent SDK** | **OpenAI Agents Runner** | **Pi** | **Flue** | **Google ADK** |
|---|---|---|---|---|---|
| **Provider model** | Env-var dispatch in bundled `claude` binary; `model` is a string | `ModelProvider.get_model(name)`; Responses or Chat Completions; LiteLLM bridge | `ModelRegistry` + `AuthStorage`; prefix-resolved | `registerProvider(name, opts)`; prefix-routed model strings | `model` string or `LLM` instance; `LLMRegistry`; LiteLLM bridge |
| **Session model** | JSONL on disk, keyed by `cwd`; continue/resume/fork; queryable helpers | `Session` protocol (`get/add/pop/clear`); SQLite or in-memory | Session **tree** (parent-child branching), in-memory or persistent | Harness → session → task; Durable Object (CF) / in-memory (Node) | `Session` = `EventLog` + scoped `State`; `SessionService` (mem/DB/Vertex) |
| **Control plane** | `interrupt`, `setModel`, `setPermissionMode`, `rewindFiles`, `stopTask`, `streamInput` | None on `Runner` (cancel + re-run) | `steer` (now), `followUp` (queued), model/thinking switch, `subscribe` | `session.prompt` / `skill` / `task` (no interrupt verb) | Event-iterator from `Runner.run`; no side-channel verbs |
| **Sandbox** | Built-in `Bash` on host; permission modes gate it | None (tools are your functions) | Built-in shell tools on host; per-`cwd` tool factories | Virtual `just-bash` (default), `local`, or container (Daytona/E2B) | None built-in (tools are functions / hosted Gemini tools) |
| **Extensibility** | In-process MCP tools, sub-agents (`AgentDefinition`), skills, slash commands, hooks, `.claude/` discovery | `function_tool`, handoffs, guardrails, tracing spans | `defineTool` (TypeBox), extensions/skills/prompts, `DefaultResourceLoader`, `.pi/` discovery | Valibot tools, `connectMcpServer`, skills, `task`, markdown **connectors**, `.flue/` | `FunctionTool`, `AgentTool`, MCP/OpenAPI toolsets, `sub_agents`, workflow agents, A2A |
| **Deployment target** | Spawned subprocess (JSONL/stdio); embed in any host | In-process; you wrap it in a server | JSON-RPC subprocess (`runRpcMode`), TUI, print mode | Webhook/cron triggers; Node, Cloudflare, GitHub Actions, GitLab CI | Vertex Agent Engine, Cloud Run (FastAPI), GKE / self-host |

These are the runtimes themselves. The `HarnessAgent` adapter layer in §7 sits
*above* this table — a single interface that drives several of these columns
(Claude Code, Codex, Pi) and normalizes them onto common streaming/session
primitives. (Capability-matrix details for each — type-safety, streaming union
shape, caching support — are tabulated in
[`05-cross-cutting.md`](05-cross-cutting.md).)

**Not every "framework" is a harness.** Broad agent frameworks such as the
**Vercel AI SDK**, **Mastra** (TS), and **Pydantic AI** (Python) sit at the
*composition* tier, not this one: they give you the loop, tools, typed
agents, structured output, and (for Mastra/Pydantic AI) workflows and voice —
but they do **not** ship the heavyweight runtime surface this file is about
(a bundled long-lived process, an opinionated coding toolset, an OS-level
sandbox, on-disk session transcripts, a control plane like `interrupt`/`rewind`).
They are covered where their capabilities live:
[`02`](02-tools-and-agents.md) (loop, tools, multi-agent),
[`04`](04-realtime-and-transports.md) (Mastra voice),
[`05`](05-cross-cutting.md) (Pydantic Evals/Logfire), and
[`07`](07-workflows-and-orchestration.md) (Mastra workflows, pydantic-graph).
The line is fuzzy — Mastra has a dev server and deploy targets — but the
distinguishing trait of a harness is *owning a running, sandboxed workspace you
steer over time*, not *helping you assemble a call or a graph*.

---

## What varies / what's hard (callout)

Pulling the threads together — the structural difficulties any layer that wraps
or embeds a whole agent runtime has to absorb:

- **Driven, not called.** A harness is operated over time, so the host needs a
  *handle* with a control plane (interrupt / steer / setModel / rewind), and the
  runtime has to land side-channel commands at safe boundaries while a turn is
  streaming. Runtimes disagree on whether the control plane even exists: rich on
  the Claude `Query` and Pi `AgentSession`, absent on the OpenAI `Runner`.

- **The fence is the sandbox, not the allow-list.** Tool allow/deny and
  command patterns (`Bash(rm*)`) are sieves; real safety comes from *where* the
  shell runs — virtual (`just-bash`), local, or container. The same `Bash` tool
  spans a spectrum of isolation, and the agent usually can't tell which it's in.

- **Sessions are files with a working-directory dependency.** On-disk JSONL
  (Claude) and session trees (Pi) are debuggable and forkable, but `cwd`-keyed
  storage orphans transcripts when a project moves, and fork-as-copy is
  quadratic where pointer-to-parent is linear. The protocol-backed alternatives
  (Agents `Session`, ADK `SessionService`) are swappable but less inspectable.

- **Extensibility surfaces don't agree on anything.** Tools are in-process MCP
  four-tuples / `defineTool` / bare functions / Valibot schemas; sub-agents are a
  built-in `Agent` tool / a list field / a `task()` primitive; resources are
  discovered from `.claude/` vs `.pi/` vs `.flue/` with per-runtime precedence.
  Tool *output* typing is loose everywhere.

- **Deployment and triggering live outside the SDK.** Only Flue lifts
  webhook/cron triggers and runtime-agnostic build targets into the agent
  declaration; everywhere else the host re-implements serving, scheduling,
  idempotency, durable instance identity, and (for subprocess harnesses) child
  process supervision and version skew.

- **Provider selection is the weakest seam.** It ranges from brittle env-var
  dispatch inside a bundled binary (`CLAUDE_CODE_USE_BEDROCK=1`) to
  prefix-routed model strings to runtime `registerProvider` — and none of it is
  discoverable from the harness API or variable per call the way a declarative
  client block would be.
