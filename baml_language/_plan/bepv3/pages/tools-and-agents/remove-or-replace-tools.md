# Remove or replace tools

An Agent may change the complete application-tool roster before the next
provider step.

## Utilities used

| Utility | Purpose |
| --- | --- |
| `ToolRegistry.remove` | Removes one named tool |
| `ToolRegistry.replace` | Replaces one named handler |
| `StepPlan.tools` | Replaces the complete next-step roster |

## Example

```baml
class Resolution {
  reply: string,
}

function lookup_order(order_id: string) -> string {
  "out for delivery"
}

function issue_refund(order_id: string) -> string {
  "refund submitted"
}

function ResolveTicket(message: string) -> Resolution {
  provider: "openai/gpt-5.6-luna"
  prompt: `
    Resolve this support ticket.

    ${message}

    ${ctx.output_format}
  `
  tools: [lookup_order, issue_refund]
}

let registry = ai.ToolRegistry.new([
  lookup_order,
  issue_refund,
]);

registry.remove("issue_refund");

let outcome = ResolveTicket.task("Where is order-42?").run(
  runner = ai.run.Agent.new(
    tool_registry = registry,
  ),
)
```

Because `tool_registry` is supplied, it is the authoritative roster. The
Agent does not re-add the task's declared tools at startup.

`null` means keep the current roster. `[]` means remove every application
tool. A replacement persists until another explicit change.

Provider-owned tools are changed by selecting provider configuration with
those tools enabled or disabled; they are not entries in the application
registry.

[Back to tools and agents](../tools-and-agents.md)
