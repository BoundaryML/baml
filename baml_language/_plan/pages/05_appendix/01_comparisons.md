# Comparisons

How the concepts in this BEP map onto other systems. The goal is
orientation, not scorekeeping.

## Pydantic AI

| Pydantic AI | BAML |
|---|---|
| `Agent('openai:gpt-5.2', output_type=Itinerary)` | `function PlanTrip(...) -> Itinerary { client: ... }` |
| `@agent.tool` decorated function | any function in `tools: [...]` |
| `system_prompt=` string in Python | `prompt:` block, compile-checked |
| `agent.run_sync(...)` → `result.output` | `PlanTrip(...)` → the value |
| `message_history=result.new_messages()` | session-managed; `${ctx.transcript}` |
| `ModelMessagesTypeAdapter` persistence | `s.snapshot()` → one string |
| `RunContext` / `deps` | closures capture what they need |

Differences that matter: BAML persists typed events (the journal) rather
than provider messages, which is what enables cross-provider resume and
built-in tracing; `Done | Replied` distinguishes goal-reached from
conversation-continuing, which `run()`-always-has-output does not; prompts
and schemas are checked at compile time.

## OpenAI Agents SDK

| OpenAI Agents SDK | BAML |
|---|---|
| `Agent` + instructions | LLM function |
| `Runner.run(agent, input)` | `run()` on a session (the runner is internal) |
| `final_output` | `Done<T>.result` |
| `Session` | session with a journal |
| handoffs | subagent calls |
| guardrails | policies / middleware |

The SDK ties session state to its own storage and provider; BAML sessions
serialize to a string and re-render per provider.

## Flue

Flue is the closest system in operational rigor. Shared conclusions,
reached independently: a durable canonical stream per conversation with a
separate ephemeral observability stream; subagents as isolated child
sessions with their own durable records; tool argument validation before
execution; tool errors returned to the model; async submission with
receipts and exactly-one-settlement.

Where the designs diverge:

- **Typed goals.** A Flue agent function returns its system prompt; the
  conversation has no typed outcome. A BAML agent returns `T`, and the
  loop's termination condition is producing it.
- **Re-render vs. policy.** Flue re-runs the agent function before every
  model call; capabilities are mounted conditionally during render
  (`if (approved) useTool(publish)`). BAML keeps the function static and
  makes capability changes policy commands recorded in the journal
  (`MountTools`), so the cause of every change is in the history. See
  `02_alternatives_considered.md`.
- **Extension seam.** Flue's harness loop is fixed; configuration happens
  through hooks. BAML exposes the policy layer, so injection timing,
  approvals, and budgets are user-definable and testable.
- **State location.** Flue conversations live in a database adapter; BAML
  journals are values that serialize to a string, with stores as an
  option for named instances.

Adopted from Flue's design, with credit: atomic commit seams for state,
receipts and settlement outcomes, named conversation instances with
create-only semantics, and step checkpoints inside durable tools.

## pi

pi is a coding agent whose SDK (`@earendil-works/pi-coding-agent`)
embeds the agent in applications. Of the systems here it is the closest
to this BEP's session design: one entry point, extension through option
slots, and an append-only session file as the source of truth.

| pi | BAML |
|---|---|
| `createAgentSession({ ... })` | `f@session(args, $config...)` |
| `SessionManager.inMemory()` / `open(path)` / `continueRecent(cwd)` | `store`, `id`, `resume` options |
| `ModelRuntime` (models + auth) | `client<llm>` resolution |
| `defineTool({ ... })` | any function in `tools: [...]` |
| `session.prompt(text)` | `s.send(text)` then `s.run()` |
| `steer()` / `followUp()` | `send()` / `send(after_done = true)` |
| `session.subscribe(event)` | journal tail plus the ephemeral stream |
| session `.jsonl` entry tree, `fork(entryId)` | the journal; forking |
| `runPrintMode` / `runRpcMode` / `InteractiveMode` | task mode / `baml serve` / application code |
| extensions: `pi.on("agent_start", ...)` | policies |

### Session creation

Both designs use one constructor with pluggable slots rather than one
constructor per kind of session:

```typescript
// pi
const { session } = await createAgentSession({
  sessionManager: SessionManager.continueRecent(cwd),
  modelRuntime: await ModelRuntime.create(),
  customTools: [myTool],
});
await session.prompt("What files are in the current directory?");
```

```baml
// BAML
let s: Session<Report> = CodeAgent@session(
    goal = "list the files in the current directory",
    $store = file_store,
    $id = most_recent_id(),
);
let turn = s.run();
```

The slot mapping is direct: `SessionManager` is the store plus instance
semantics, `ModelRuntime` is client resolution, `customTools` is the
toolbox. One difference is where kinds of session come from: pi always
returns `AgentSession`, and jobs or custom transports are built around
it; in BAML the `runner` option changes the handle type
(`02_alternatives_considered.md`, section 1).

### Tools

```typescript
// pi — the schema is a runtime value (TypeBox), written alongside the types
const myTool = defineTool({
  name: "my_tool",
  description: "Does something useful",
  parameters: Type.Object({
    input: Type.String({ description: "Input value" }),
  }),
  execute: async (_toolCallId, params) => ({
    content: [{ type: "text", text: `Result: ${params.input}` }],
    details: {},
  }),
});
```

```baml
// BAML — the signature is the schema
/// Does something useful.
function my_tool(input: string) -> string {
    `Result: ${input}`
}
```

The difference is erasure, not design taste. A TypeScript type cannot
render a schema or validate arguments at runtime, so pi carries TypeBox
declarations that restate what the types say, and the two can drift.
BAML types are runtime values: the signature is simultaneously the
compile-time check, the schema the model sees, and the validator.

### Providers

```typescript
// pi: a provider declares metadata and reuses a wire API implementation.
setProvider(createProvider({
  id: "ollama",
  auth: { apiKey: { resolve: async () => ({ auth: {} }) } },
  models: [...],
  api: openAICompletionsApi(),
}));
```

```baml
// BAML: provider names the wire format and options configure the client.
client<llm> Local {
    provider: openai,
    options: { base_url: "http://localhost:11434/v1", model: "qwen3:32b" },
}
```

Pi separates a provider descriptor from its reusable API implementation. The
descriptor owns provider identity, authentication, model metadata, and the API
binding. The API implementation owns request conversion, transport, and stream
ingestion. An OpenAI-compatible service can therefore reuse
`openAICompletionsApi()` without implementing another agent loop.

The current BAML plan combines more of this work behind `client<llm>` and the
`Client` interface. Its `provider:` field selects a wire format, while options
select the endpoint, model, credentials, and retry policy. The current runtime
contract exposes `render`, `invoke`, and `ingest` as client methods. Whether the
service-descriptor/API-adapter split and the three phases should be public is
under reconsideration in `03_client_replay_and_continuations.md`.

### Replay and remote continuation

Pi's normal `openai-responses` adapter builds complete input from local context
and sets `store: false`. It has no dedicated `previous_response_id` or OpenAI
Conversations option. It retains response IDs, message and item IDs,
function-call IDs, encrypted reasoning items, and text phase metadata inside
its local assistant representation. The next request reconstructs OpenAI input
items from that local data. See [the normal Responses request
construction](https://github.com/earendil-works/pi/blob/6b461b75b39b5a19b378dc42fbfbd1655bc446a6/packages/ai/src/api/openai-responses.ts#L260-L335)
and [message
conversion](https://github.com/earendil-works/pi/blob/6b461b75b39b5a19b378dc42fbfbd1655bc446a6/packages/ai/src/api/openai-responses-shared.ts#L130-L290).

Pi's `openai-codex-responses` adapter has a narrower optimization. A cached
WebSocket connection stores the previous request, response items, and response
ID. When the next complete input is an extension of that prefix and the other
request fields match, the adapter replaces the prefix with
`previous_response_id` and sends only the delta. It sends full context when the
comparison or connection reuse fails. See [cached continuation
selection](https://github.com/earendil-works/pi/blob/6b461b75b39b5a19b378dc42fbfbd1655bc446a6/packages/ai/src/api/openai-codex-responses.ts#L1387-L1438).

Pi's `sessionId` is a prompt-cache, session-affinity, or connection-cache key
depending on the adapter. It is not an OpenAI conversation ID. No OpenAI
Conversations API integration is present at the reviewed commit.

This comparison exposes three separate requirements for BAML. Canonical
content provides portability. API-native replay data provides exact stateless
same-API fidelity. An optional remote continuation checkpoint can reduce a
later request to a delta. These values have different lifetimes and should not
be represented by one undifferentiated raw response field.

### Steering

```typescript
// pi
await session.steer("New instruction");                       // redirect current work
await session.followUp("After you're done, also do this");    // deliver after completion
```

```baml
// BAML
s.send("new instruction");                                    // injected at the next turn boundary
s.send("after you're done, also do this", after_done = true); // delivered after Done
```

The steer/follow-up distinction is adopted from pi. In BAML it is one
flag on `send` because injection timing is already policy behavior.

### Extensions

```typescript
// pi — imperative lifecycle hooks
const ext: InlineExtension = {
  name: "my-provider",
  factory: (pi) => {
    pi.on("agent_start", () => { console.log("starting"); });
  },
};
```

```baml
// BAML — a pure policy: events in, commands out
class Mine {
    inner: baml.session.Policy,
    implements baml.session.Policy {
        function update(self, st: SessionState, j: Journal, e: Event) -> Command[] {
            match (e) {
                let m: UserMessage => { log.info("turn starting"); [] },
                _ => self.inner.update(st, j, e),
            }
        }
    }
}
```

Hooks run effects at lifecycle points; a policy decides and the runner
acts. The policy form is unit-testable with literal events and safe
under replay; hook effects re-run unless the author guards them.

Differences that matter: a pi session has no typed goal. `prompt()` produces
text, and there is no `Done<Itinerary>`. Pi stores canonical messages and
API-specific replay metadata rather than BAML's typed event journal, so
typed journal queries are not available. Pi performs cross-provider conversion
from its canonical content, but it does not preserve BAML's tool, policy,
child-session, and typed-outcome event structure. Pi also has no policy seam.
Its loop is fixed, with hooks at its edges.

Adopted from pi, with credit: the steer/follow-up distinction, session
forking (`fork(entryId)` over the entry tree — the journal analog is a
new journal referencing a parent prefix), and `continueRecent` as a
first-class store query.

## LangGraph

LangGraph models an agent as a state graph with a checkpointer. BAML has
no graph DSL: control flow is code, and the journal — the analogue of the
checkpointer — is the primary object rather than a plugin. A graph can be
built on a log; a log cannot be recovered from a graph.
