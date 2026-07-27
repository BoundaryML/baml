# Multiple and parallel tools

One provider step may request several independent tools. The Agent may execute
them concurrently and submit one result for each call ID.

## Utilities used

| Utility | Purpose |
| --- | --- |
| `tools:` | Declares the roster |
| `ai.run.Agent` | Coordinates calls and results |
| `spawn` | Runs independent BAML work concurrently |

## Example

```baml
class Resolution {
  reply: string,
}

function lookup_order(order_id: string) -> string {
  "Order is out for delivery."
}

function search_policy(query: string) -> string {
  "Late orders qualify for review after seven days."
}

function ResolveTicket(message: string) -> Resolution {
  provider: "openai/gpt-5.6-luna"
  prompt: `
    Resolve this ticket. Use independent tools together when useful.

    ${message}

    ${ctx.output_format}
  `
  tools: [lookup_order, search_policy]
}

let outcome = ResolveTicket.task("My order is late.").run(
  runner = ai.run.Agent.new(),
)
```

If the provider asks for both tools in one step, the Agent may run both before
calling `provider.submit`.

```text
call-1 lookup_order  ─┐
                      ├─ run concurrently ─ submit [result-1, result-2]
call-2 search_policy ─┘
```

Result order and provider call IDs remain stable even when execution finishes
in another order. A concurrency limit may be supplied through runner policy
without changing the tool functions.

[Back to tools and agents](../tools-and-agents.md)
