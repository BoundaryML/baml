# Change tools between turns

A registry lets hooks add or remove tools between provider steps.

## Utilities used

| Utility | Purpose |
| --- | --- |
| `ai.ToolRegistry` | Holds the active application tools |
| `StepContext.tools` | Read-only snapshot of the current roster |
| `StepPlan.tools` | Complete replacement roster for the next step |

## Example

```baml
class Resolution {
  reply: string,
}

function lookup_account(customer_id: string) -> string {
  "active"
}

class AccountToolDiscovery {
  registry: ai.ToolRegistry,

  function discover(self) -> string {
    self.registry.add(lookup_account);
    "account tools enabled"
  }
}

function ResolveTicket(message: string) -> Resolution {
  provider: "openai/gpt-5.6-luna"
  prompt: `
    Resolve this ticket. Discover more tools when necessary.

    ${message}

    ${ctx.output_format}
  `
}

let registry = ai.ToolRegistry.new([]);
let discovery = AccountToolDiscovery { registry: registry };
registry.add(discovery.discover);

let outcome = ResolveTicket.task("Check customer customer-7.").run(
  runner = ai.run.Agent.new(
    tool_registry = registry,
  ),
)
```

Calling `discovery.discover` adds `lookup_account`. The current request keeps
the roster it was sent; the new tool appears on the next provider step and
persists.

Tool names are unique. Adding the same function again is idempotent. Adding a
different handler under an existing name fails instead of silently shadowing
it. Replacement is explicit.

[Back to tools and agents](../tools-and-agents.md)
