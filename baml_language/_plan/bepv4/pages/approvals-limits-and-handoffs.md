# Approvals, limits, and handoffs

Prompts guide model behavior. Agent callbacks enforce application policy. Pass
approval, authorization, argument rewriting, and blocking logic directly to
`Agent.new(...)`.

## Utilities used

| Utility | What it does |
| --- | --- |
| `before_tool_call` callback | Makes a decision before an Agent runs a tool |
| `prepare_step` callback | Changes the next provider, tool roster, or stop decision |
| `ai.tools.ToolDecision` | Allows, replaces, or blocks one tool call |
| `ai.Budget { max_steps, max_cost_usd }` | Stops work between provider steps |
| `max_tool_calls_per_step` | Rejects an oversized provider tool batch before any tool effect |
| `ai.tools.tool(...).as_handoff()` | Returns one exact call to the application before dispatch |

## Example

```baml
enum TicketPriority {
  Low
  Normal
  Urgent
}

class Resolution {
  category: string,
  priority: TicketPriority,
  summary: string,
  reply: string,
}

/// Look up an account with optional history.
function lookup_account_with_history(
  customer_id: string,
  include_history: bool = false,
) -> json throws never {
  { "customer_id": customer_id, "include_history": include_history }
}

/// Look up a customer account.
function lookup_account(customer_id: string) -> json throws never {
  { "customer_id": customer_id, "status": "active", "tier": "pro" }
}

function ResolveTicketWithTools(ticket: SupportTicket) -> Resolution {
  provider: "openai-responses/gpt-5.6-luna"
  prompt: `
    Resolve ticket ${ticket.id}. Use the available tools before answering.
    Hand the account over to a person when the request needs authority you
    do not have.

    ${ctx.output_format}
  `
  tools: [
    lookup_account_with_history,
    ai.tools.tool(lookup_account).as_handoff(),
  ]
}

let history_approved = false;

let outcome = ResolveTicketWithTools@task(sample_ticket()).run(
  runner = ai.run.Agent<Resolution>.new(
    before_tool_call = (event) -> {
      if (event.call.name == "lookup_account_with_history" && !history_approved) {
        ai.tools.ToolDecision.block("human approval required")
      } else {
        ai.tools.ToolDecision.allow(event.call)
      }
    },
    budget = ai.Budget { max_steps: 8, max_cost_usd: 0.25 },
    max_tool_calls_per_step = 1,
  ),
)
```

### What happens

```mermaid
flowchart TD
  task["ResolveTicketWithTools task"] --> budget{"Step and cost budget remain?"}
  budget -->|yes| model["Provider step"]
  model --> result{"Provider returned?"}
  result -->|final value| done["Done<Resolution>"]
  result -->|tool calls| callLimit{"Within per-step call limit?"}
  callLimit -->|no| limitFailure["ToolCallLimitExceeded with pending calls + conversation"]
  callLimit -->|yes| transfer{"Handoff tool?"}
  transfer -->|yes| handoff["Handoff with exact ToolCall"]
  transfer -->|no| callback["before_tool_call"]
  callback -->|approved| history["Run lookup_account_with_history"]
  callback -->|not approved| blocked["Return blocked tool result"]
  history --> submit["Submit correlated result"]
  blocked --> submit
  submit --> budget
  budget -->|no| stopped["BudgetReached"]
```

### Illustrative output

```console
[INFO] proposed tool: lookup_account_with_history(customer_id = "C-1", ...)
[INFO] before_tool_call: blocked "human approval required"
[INFO] returned blocked result to the model
[INFO] Agent returned Handoff { call: ToolCall { id: "call_42", name: "lookup_account", ... }, ... }
```

A blocked call still receives a correlated tool result. The model can explain
the denial, choose another action, or request a handoff. The blocked function
does not run.

A handoff tool never runs inside the Agent. When the model calls a tool marked
`.as_handoff()`, the Agent returns `ai.Handoff` with the exact `ToolCall`
before dispatch. The call retains its provider correlation ID, name, and
arguments.

A handoff must be unambiguous. If one model step mixes a handoff with an
application call, or returns several handoff calls, the Agent throws
`ai.InvalidRequest` before executing anything.

## Enforce the number of tool calls in one step

Provider wire options such as `parallel_tool_calls = false` tell a model how
it should respond. They are not an application policy boundary: a provider,
model, proxy, or future adapter can still return an unexpected batch. Put the
hard limit on the runner:

```baml
let outcome = ResolveTicketWithTools@task(sample_ticket()).run(
  runner = ai.run.Agent<Resolution>.new(
    max_tool_calls_per_step = 1,
  ),
)
```

`max_tool_calls_per_step` has these exact semantics:

- `null` (the default) accepts any non-empty, valid batch and preserves the
  existing parallel-tool behavior.
- `0` rejects every tool request while still allowing a final `T`.
- `N` accepts at most `N` calls from one provider `step`.
- Handoff calls count. Therefore a limit of zero rejects a handoff, while a
  limit of one permits one unambiguous handoff.
- Negative values are invalid configuration and fail before `provider.begin`.

The Agent first validates the provider's correlation envelope, then checks
the count. On a count violation it has not emitted a tool-call event, invoked
`before_tool_call`, delivered a handoff, run a handler, or invoked
`after_tool_call`.

The typed `ai.ToolCallLimitExceeded` failure carries:

| Field | Meaning |
| --- | --- |
| `provider` | Provider that returned the batch |
| `max_tool_calls_per_step` | Configured runner limit |
| `actual_tool_calls` | Number returned by this step |
| `calls` | Exact pending calls, including correlation IDs |
| `conversation` | Provider-owned continuation at the pending-call boundary |
| `steps_taken` | Number of completed provider steps |

It is non-transient with `Effects.None`: retrying the same provider step does
not fix a deterministic policy mismatch, and no application tool effect has
occurred. It is a failure rather than `BudgetReached` because the provider
conversation contains unresolved calls. The application must explicitly
accept or reject every call before taking the next model step.

For example, an application can reject the batch and resume without replaying
the model step:

```baml
let outcome = ResolveTicketWithTools@task(sample_ticket()).run(
  runner = ai.run.Agent<Resolution>.new(max_tool_calls_per_step = 1),
) catch (e) {
  let limited: ai.ToolCallLimitExceeded => {
    let provider = match (limited.conversation.provider()) {
      let agent: ai.AgentProvider => agent,
      _ => throw baml.errors.Unsupported {
        message: "conversation provider cannot resume an Agent",
      },
    };
    let rejected = limited.calls.calls.map((call) -> {
      ai.tools.ToolResult.error(call, "only one tool call is allowed per step")
    });
    let continued = provider.submit(limited.conversation, rejected);

    ResolveTicketWithTools@task(sample_ticket()).run(
      runner = ai.run.Agent<Resolution>.new(
        max_tool_calls_per_step = 1,
        conversation = continued,
      ),
    )
  },
}
```

Submitting correlated errors is one policy choice. An application may instead
inspect and fulfill the pending calls externally, but it must preserve every
call ID and acknowledge the complete batch before resuming.

## Handle every terminal outcome

Each outcome carries what the caller needs to continue:

- `ai.Done<T> { value, metadata, conversation }` — the final typed value, the
  response metadata, and the conversation that produced it.
- `ai.BudgetReached { conversation, steps_taken, reason }` — a safe stop with
  everything needed to resume. `Budget` is multi-dimensional — `max_steps`
  and/or `max_cost_usd` — and `reason` names the limit that tripped.
- `ai.Handoff { call, conversation, steps_taken }` — a tool marked
  `.as_handoff()` fired; `call` is the exact `ai.tools.ToolCall` the
  application must resolve.
- `ai.Interrupted { conversation, steps_taken, reason }` — cooperative
  cancellation reached a committed boundary with no unsubmitted application
  tool results.

```baml
match (outcome) {
  let done: ai.Done<Resolution> => log.info(done.value.reply),

  let stopped: ai.BudgetReached => {
    // Save stopped.conversation and resume after the limit or approval changes.
    log.info(`stopped after ${stopped.steps_taken} steps: ${stopped.reason}`)
  },

  let handoff: ai.Handoff => {
    // The application takes over the exact correlated call.
    log.info(`handoff requested: ${handoff.call.name} (${handoff.call.id})`)
  },

  let interrupted: ai.Interrupted => {
    // This conversation can resume directly with a fresh cancellation token.
    log.info(`interrupted after ${interrupted.steps_taken} steps`)
  },
}
```

All incomplete outcomes retain the conversation. `BudgetReached` and
`Interrupted` can resume that state directly. A `Handoff` still has a pending
provider call, so the application must submit its result first.

## Complete a handoff and resume

The application performs the external work, creates one result correlated to
`handoff.call.id`, and submits it through the conversation's provider:

```baml
function resume_after_handoff(
  handoff: ai.Handoff,
  external_output: json,
) -> ai.Done<Resolution> | ai.BudgetReached | ai.Handoff | ai.Interrupted
    throws ai.Failure | baml.errors.UnknownError | baml.errors.Unsupported {
  let provider = match (handoff.conversation.provider()) {
    let agent: ai.AgentProvider => agent,
    _ => throw baml.errors.Unsupported {
      message: "handoff conversation provider cannot resume an Agent",
    },
  };

  let result = ai.tools.ToolResult.ok(handoff.call, external_output);
  let conversation = provider.submit(handoff.conversation, [result]);

  ResolveTicketWithTools@task(sample_ticket()).run(
    runner = ai.run.Agent<Resolution>.new(
      conversation = conversation,
    ),
  )
}
```

`ToolResult.ok(handoff.call, ...)` copies the exact call ID. Use
`ToolResult.error(handoff.call, message)` when the external work fails. In
either case, `submit` must happen before the next Agent `step`.

## What callbacks can change

| Callback | Common decisions |
| --- | --- |
| `before_tool_call` | Allow, rewrite arguments, replace, or block |
| `after_tool_call` | Record, redact, or normalize the result |
| `prepare_step` | Change the tool roster, switch providers, or request a safe stop |

Observers are different: they can log the same events, but they cannot change
execution.
