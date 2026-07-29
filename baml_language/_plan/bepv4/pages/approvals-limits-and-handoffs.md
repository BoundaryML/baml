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
  result -->|tool call| transfer{"Handoff tool?"}
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
}
```

Both incomplete outcomes retain the conversation. `BudgetReached` can resume
that state directly. A `Handoff` still has a pending provider call, so the
application must submit its result first.

## Complete a handoff and resume

The application performs the external work, creates one result correlated to
`handoff.call.id`, and submits it through the conversation's provider:

```baml
function resume_after_handoff(
  handoff: ai.Handoff,
  external_output: json,
) -> ai.Done<Resolution> | ai.BudgetReached | ai.Handoff
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
