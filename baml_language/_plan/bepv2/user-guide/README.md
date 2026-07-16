# BEP-064 user guide

> **Status:** DRAFT — this guide demonstrates the proposed BEP-064 surface. The
> normative contracts remain in [`../pages`](../pages/).

The design pages answer “what is the contract?” This guide answers “how do I
build something with it?” Examples begin with application usage, add one idea
at a time, and show only the implementation seam needed for that idea.

Most chapters extend the same customer-support application described in
[`00-running-example.md`](./00-running-example.md). Stable names and data types
make the differences between drivers, tools, providers, and resources visible;
the reader should not have to learn a new toy domain on every page.

## Recommended path

Read these topics in order for the shortest path from one typed call to a
production agent:

```text
direct typed call
  -> Task<T, P> and an explicit driver
  -> provider override and direct provider access
  -> one tool
  -> complete agent loop
  -> multiple and parallel tools
  -> events and hooks
  -> dynamic tools and MCP
  -> provider switching
  -> conversation and session state
  -> media and realtime resources
  -> retry, fallback, and routing
  -> provider matrix tests and evaluation
  -> external harnesses
```

Extraction and classification in the first chapter are **model calls**, not
tool calls. A tool call begins when the model asks application or provider code
to perform another operation such as `lookup_order`.

## Organization

The guide is organized by the questions users search for, rather than as one
global list of dozens of unrelated scenarios. Numbers establish the order of
themes; pages inside each theme add concepts progressively.

Start with the [running customer-support example](./00-running-example.md),
then follow a theme or jump directly to a recipe.

### Snippet convention

Interface methods are always shown inside the class and `implements` block
that own them. For example, `prepare_step` appears inside
`implements ai.AgentHooks`; it is not a top-level function. Comments containing
`...` mark fields or other interface methods omitted from that particular
example:

```baml
class ExampleHooks {
  // ...policy fields...

  implements ai.AgentHooks {
    function prepare_step(self, ctx: ai.StepContext) -> ai.StepPlan throws never {
      // ...decision logic...
      ai.StepPlan { provider: null, tools: null, stop: null }
    }

    // ...other AgentHooks methods use their defaults...
  }
}
```

### 1. Tasks and providers

1. [Direct typed call](./01-tasks-and-providers/01-direct-typed-call.md)
2. [Task and drivers](./01-tasks-and-providers/02-task-and-drivers.md)
3. [Streaming and metadata](./01-tasks-and-providers/03-streaming-and-metadata.md)
4. [Provider overrides](./01-tasks-and-providers/04-provider-overrides.md)
5. [Direct provider access](./01-tasks-and-providers/05-direct-provider-access.md)

### 2. Tools and agents

1. [One tool](./02-tools-and-agents/01-one-tool.md)
2. [Agent loop](./02-tools-and-agents/02-agent-loop.md)
3. [Multiple and parallel tools](./02-tools-and-agents/03-multiple-and-parallel-tools.md)
4. [Events and hooks](./02-tools-and-agents/04-events-and-hooks.md)
5. [Modify or block tool calls](./02-tools-and-agents/05-modify-or-block-tool-calls.md)
6. [Dynamic tool registry](./02-tools-and-agents/06-dynamic-tool-registry.md)
7. [MCP halfway through a loop](./02-tools-and-agents/07-mcp-mid-loop.md)
8. [Handoffs and budgets](./02-tools-and-agents/08-handoffs-and-budgets.md)
9. [Remove tools between agent steps](./02-tools-and-agents/09-remove-tools.md)

### 3. Routing and reliability

1. [Retry safe calls](./03-routing-and-reliability/01-retry-safe-calls.md)
2. [Fallback between providers](./03-routing-and-reliability/02-fallback-between-providers.md)
3. [Route before the call](./03-routing-and-reliability/03-route-before-the-call.md)
4. [Switch provider between turns](./03-routing-and-reliability/04-switch-provider-between-turns.md)
5. [Switch provider after failure](./03-routing-and-reliability/05-switch-provider-after-failure.md)
6. [Side effects and idempotency](./03-routing-and-reliability/06-side-effects-and-idempotency.md)
7. [Streaming failure boundaries](./03-routing-and-reliability/07-streaming-failure-boundaries.md)

### 4. Conversations and state

1. [Application-owned history](./04-conversations-and-state/01-application-owned-history.md)
2. [Compaction and memory](./04-conversations-and-state/02-compaction-and-memory.md)
3. [Fork a conversation](./04-conversations-and-state/03-fork-a-conversation.md)
4. [Provider-owned sessions](./04-conversations-and-state/04-provider-owned-sessions.md)
5. [Save and resume](./04-conversations-and-state/05-save-and-resume.md)

### 5. Media and realtime

1. [Images, PDFs, and audio](./05-media-and-realtime/01-images-pdfs-and-audio.md)
2. [Bounded audio streams](./05-media-and-realtime/02-bounded-audio-stream.md)
3. [Realtime channels](./05-media-and-realtime/03-realtime-channel.md)
4. [Barge-in](./05-media-and-realtime/04-barge-in.md)
5. [Realtime tools](./05-media-and-realtime/05-realtime-tools.md)

### 6. Observability and testing

1. [Observe a call](./06-observability-and-testing/01-observe-a-call.md)
2. [Observe an agent](./06-observability-and-testing/02-observe-an-agent.md)
3. [Usage and cost](./06-observability-and-testing/03-usage-and-cost.md)
4. [Test one task across providers](./06-observability-and-testing/04-test-one-task-across-providers.md)
5. [Evaluate provider quality](./06-observability-and-testing/05-evaluate-provider-quality.md)
6. [Fakes and failure injection](./06-observability-and-testing/06-fakes-and-failure-injection.md)

### 7. Production

1. [Background jobs](./07-production/01-background-jobs.md)
2. [Caching](./07-production/02-caching.md)
3. [Deployment and transports](./07-production/03-deployment-and-transports.md)
4. [Capability negotiation](./07-production/04-capability-negotiation.md)

### 8. External harnesses

1. [Call a coding harness](./08-external-harnesses/01-call-a-coding-harness.md)
2. [Stream harness events](./08-external-harnesses/02-stream-harness-events.md)
3. [Permissions and sandboxes](./08-external-harnesses/03-permissions-and-sandboxes.md)
4. [Steer and interrupt](./08-external-harnesses/04-steer-and-interrupt.md)
5. [Save and resume a harness](./08-external-harnesses/05-save-and-resume-harness.md)
6. [Implement a harness adapter](./08-external-harnesses/06-implement-a-harness-adapter.md)

## Page format

Every example page uses this order when the sections apply. Short conceptual
recipes may omit a section rather than repeat code that adds no new behavior.

### 1. Use it

Start with the application code a user writes:

```baml
let result = ResolveTicket(ticket)
```

or:

```baml
let run = ai.drivers.run_agent(
  ResolveTicket.task(ticket),
  ai.AgentOptions { tools: [lookup_order] },
)
```

### 2. What changed

Show the smallest meaningful diff from the preceding example:

```diff
- let result = ResolveTicket(ticket)
+ let task = ResolveTicket.task(ticket)
+ let run = ai.drivers.run_agent(
+   task,
+   ai.AgentOptions { tools: [lookup_order] },
+ )
```

### 3. Add the implementation

Show only the newly required handler, hook, driver, wrapper, or `implements`
block. Do not repeat a complete provider adapter or agent loop unless that loop
is the subject of the page.

### 4. State ownership and failure behavior

Each page identifies who owns the relevant state:

```text
task:        prompt and declared output type
driver:      lifecycle, loop, and termination
provider:    wire protocol and exact transcript
application: tool handlers, UI, logs, and business state
```

Reliability pages additionally state whether the operation may be replayed,
whether any effect may have committed, and whether output has already been
observed.

### 5. Test it

Show one deterministic test. Credentialed provider tests are separately
labelled live tests. A quality evaluation is not presented as a unit test.

### 6. Escape hatch

End with the next lower supported layer, for example:

```baml
let task = ResolveTicket.task(ticket, $provider = model)
let response = model.generate<Resolution>(task)
```

This makes the abstraction boundary visible without encouraging raw vendor
HTTP requests.

### 7. Related scenarios

Link to the exhaustive `ns_ai_scenarios` coverage. The guide teaches common
patterns; scenarios remain the edge-case and compatibility matrix.

## Reliability questions get separate pages

These are different operations and must not be collapsed into one generic
“try another client” example:

- **retry** repeats the same operation on the same provider when replay policy
  and typed failure predicates permit it;
- **fallback** rebinds a bounded task to another provider before observable
  output or committed effects make replay unsafe;
- **planned mid-loop switching** uses `prepare_step`, transcript conversion,
  and a `ProviderChanged` event; and
- **failure-driven mid-loop switching** needs an explicit driver decision that
  checks replay policy, effectfulness, observed output, and transcript-import
  fidelity before continuing.

The current BEP specifies planned switching but does not yet give
`AgentHooks` a failure decision such as `on_model_failure`. The
`switch-provider-after-failure` example must not pretend this is solved: it
should either motivate that API or use a custom driver until the normative
contract is decided.

## Testing one task across providers

Provider comparison has two separate goals:

1. **Contract tests** run the same task and provider-independent assertions
   against every `DriveProvider` under test. They answer whether the provider
   can produce the declared type and required metadata.
2. **Evaluations** compare output quality, latency, usage, and cost. They answer
   which provider is preferable for this task; they are not ordinary pass/fail
   unit tests.

Provider-specific tests should exist only where capabilities differ. The
shared matrix should keep the task, inputs, and assertions fixed.

## Relationship to the scenario corpus

The existing scenarios coalesce into these guide themes:

| Scenarios | Guide theme |
| --- | --- |
| 01–08 | tasks, typed outputs, streaming, media, and metadata |
| 09–16 | tools, agent loops, dynamic rosters, handoffs, and safety |
| 17–21 | history, compaction, branches, sessions, and memory |
| 22–25 | realtime voice, barge-in, realtime tools, and voice pipelines |
| 26–36 | transports, jobs, providers, reliability, routing, caching, observability, evals, and capabilities |
| 37–42 | external harnesses, permissions, hooks, sessions, deployment, and abstraction |

The guide intentionally has fewer conceptual examples than the scenario
corpus. Several scenarios should contribute to one coherent page rather than
forcing readers to learn a new data model for every edge case.

Workflow scenarios 43–47 remain outside this user guide. Their durable
execution contract is still being designed and should not be taught as a
settled user-facing pattern here.
