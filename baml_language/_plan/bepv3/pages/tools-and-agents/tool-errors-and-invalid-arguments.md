# Tool errors and invalid arguments

The Agent distinguishes a malformed model call from a failure inside the
application tool.

## Utilities used

| Utility | Purpose |
| --- | --- |
| `reflect.call_any` | Calls the retained function with checked named arguments |
| `reflect.InvalidArgumentError` | Describes schema or argument mismatch |
| `ai.ToolResult` | Correlates output or error with the provider call ID |

## Example

```baml
class Resolution {
  reply: string,
}

function lookup_order(
  order_id: string,
  include_history: bool = false,
) -> string {
  `order=${order_id}, include_history=${include_history}`
}

function ResolveTicket(message: string) -> Resolution {
  provider: "openai/gpt-5.6-luna"
  prompt: `
    Resolve this ticket with the available tools.

    ${message}

    ${ctx.output_format}
  `
  tools: [lookup_order]
}

let outcome = ResolveTicket.task("Check order order-42.").run(
  runner = ai.run.Agent.new(),
)
```

`include_history` is optional in the model-visible schema because the BAML
function has a default. If the model sends `order_id` as an object instead of
a string, the Agent returns a correlated invalid-argument result so the model
may repair the call.

Authentication failures, policy denials, and application exceptions retain
their typed error identity. They are not mislabeled as schema errors.

[Back to tools and agents](../tools-and-agents.md)
