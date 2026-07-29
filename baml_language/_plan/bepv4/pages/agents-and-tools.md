# Agents and tools

Use an ordinary BAML function as a tool. Its documentation tells the model
when to use it, its parameters become the input schema, and its return value
goes back to the model.

## Utilities used

| Utility | What it does |
| --- | --- |
| `tools: [...]` | Declares the LLM function's default tools |
| `ai.run.Agent` | Runs the provider and application tools in a loop |
| `ai.Done<T> \| ai.BudgetReached \| ai.Handoff` | The explicit Agent outcome union |

## Example

```baml
enum TicketPriority {
  Low
  Normal
  Urgent
}

class SupportTicket {
  id: string,
  subject: string,
  body: string,
  customer_tier: string,
}

class Resolution {
  category: string,
  priority: TicketPriority,
  summary: string,
  reply: string,
}

/// Search the support knowledge base.
function search_knowledge(query: string) -> json throws never {
  { "query": query, "article": "Duplicate charges are normally pending authorizations." }
}

/// Look up a customer account.
function lookup_account(customer_id: string) -> json throws never {
  { "customer_id": customer_id, "status": "active", "tier": "pro" }
}

function ResolveTicketWithTools(ticket: SupportTicket) -> Resolution {
  provider: "openai-responses/gpt-5.6-luna"
  prompt: `
    Resolve ticket ${ticket.id}. Use the available tools before answering.

    ${ctx.output_format}
  `
  tools: [
    search_knowledge,
    lookup_account,
  ]
}

let resolution: Resolution = ResolveTicketWithTools(sample_ticket())
```

### What happens

```mermaid
flowchart TD
  call["ResolveTicketWithTools(ticket)"] --> budget{"Default loop limit remains?"}
  budget -->|yes| model["Provider reads prompt and tool schemas"]
  model --> result{"Provider returned?"}
  result -->|tool calls| validate["Validate named arguments"]
  validate --> tool["Run application functions"]
  tool --> submit["Submit correlated results"]
  submit --> budget
  result -->|final value| done["Typed Resolution"]
  budget -->|no| error["Direct call fails: no final value"]
```

### Illustrative output

```console
[INFO] ResolveTicketWithTools started
[INFO] called tool: search_knowledge(query = "duplicate charge")
[INFO] tool returned: { "query": "duplicate charge", "article": "Duplicate charges are normally pending authorizations." }
[INFO] ResolveTicketWithTools returned Resolution { category: "billing", ... }
```

The direct call is usually enough. If the model requests `search_knowledge`,
BAML validates its named arguments, invokes the real function, sends the
result back, and continues until it has a valid `Resolution`.

The generated direct call uses a default `ai.run.Agent` internally. The
provider returns `ToolCalls`; it never invokes `search_knowledge` itself. The
Agent owns argument validation, `reflect.call_any`, result correlation, and
the next provider step.

The model cannot choose captured values or bound `self`. This makes closures
and bound methods useful for tenant-scoped tools:

```baml
class BoundAccountTools {
  tenant_id: string,

  /// Look up an account with a bound application service.
  function lookup(self, customer_id: string) -> json throws never {
    { "tenant_id": self.tenant_id, "customer_id": customer_id }
  }
}

let tenant_accounts = BoundAccountTools { tenant_id: "tenant-42" };

let outcome = ResolveTicketWithTools@task(sample_ticket()).run(
  runner = ai.run.Agent<Resolution>.new(
    tools = [
      tenant_accounts.lookup,
      search_knowledge,
    ],
  ),
)
```

### Tenant-scoped tool flow

```mermaid
flowchart TD
  tenant["captured tenant_id"] --> bound["tenant_accounts.lookup"]
  task["ResolveTicketWithTools task"] --> agent["Agent with tool override"]
  bound --> agent
  agent --> budget{"Budget remains?"}
  budget -->|yes| step["Provider step"]
  step --> result{"Final value or tool calls?"}
  result -->|tool calls| method["BoundAccountTools.lookup"]
  method --> scoped["Tenant-scoped account service"]
  scoped --> submit["Submit tool result"]
  submit --> budget
  result -->|final value| done["Done<Resolution>"]
  budget -->|no| stopped["BudgetReached"]
```

### Illustrative output

```console
[INFO] Agent tool override: lookup, search_knowledge
[INFO] called bound tool: lookup(customer_id = "C-1")
[INFO] lookup used captured tenant_id = "tenant-42"
```

Passing `tools` to `Agent.new` replaces the tools declared on the LLM
function. Omitting it inherits the declaration. Passing `tools = []` disables
application tools for that run.

## When you need the exact outcome

An explicit Agent run returns one of three outcomes:

| Outcome | Meaning |
| --- | --- |
| `ai.Done<T>` | The final typed value is ready |
| `ai.BudgetReached` | The run stopped safely and can be continued |
| `ai.Handoff` | The application should handle one exact tool call |

```baml
let outcome: ai.Done<Resolution> | ai.BudgetReached | ai.Handoff =
  ResolveTicketWithTools@task(sample_ticket()).run(
    runner = ai.run.Agent<Resolution>.new(
      budget = ai.Budget { max_steps: 8, max_cost_usd: null },
    ),
  );

match (outcome) {
  let done: ai.Done<Resolution> => log.info(done.value),
  let stopped: ai.BudgetReached => log.info(stopped.reason),
  let handoff: ai.Handoff => log.info(handoff.call.name),
}
```

Use the direct call when incomplete outcomes are exceptional. Use the explicit
runner when stop, resume, or handoff is normal application control flow.
A handoff retains the provider's call ID; submit a `ToolResult` for
`handoff.call` before resuming its conversation.

Runnable examples:

```console
baml run --from crates/baml_tests/baml_src_temp2 \
  ai_scenarios.one_tool

baml run --from crates/baml_tests/baml_src_temp2 \
  ai_scenarios.agent_loop

baml run --from crates/baml_tests/baml_src_temp2 \
  ai_scenarios.multiple_and_parallel_tools
```
