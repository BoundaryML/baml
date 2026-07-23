# Change the tool registry between turns

> **Status:** Implemented in the executable reference.

A long-running agent does not need to expose every possible tool up front. A
driver-owned `ToolRegistry` may grow or shrink between provider turns. Hooks
see a snapshot of its current tools; `StepContext` does not expose the registry
for direct mutation.

## Start with a small roster

```baml
let registry = ai.ToolRegistry.new([lookup_order])

let options = ai.AgentOptions.new(
  tool_registry = registry,
  hooks = DynamicToolHooks { enable_refunds_at_step: 2 },
)

let run = ai.drivers.run_agent(
  ResolveTicket.task(ticket, $provider = ToolModel),
  options,
)
```

`AgentOptions` is configuration, so construct the complete value once. The
registry itself is intentionally stateful. During the loop, the driver applies
the complete roster returned by the hook.

## Update the next step

```baml
class DynamicToolHooks {
  enable_refunds_at_step: int,

  implements ai.AgentHooks {
    function prepare_step(self, ctx: ai.StepContext) -> ai.StepPlan throws never {
      if (ctx.step != self.enable_refunds_at_step) {
        return ai.StepPlan { provider: null, tools: null, stop: null }
      }

      let next_tools = ctx.tools.slice(0, ctx.tools.length())
      next_tools.push(issue_refund)

      ai.StepPlan {
        provider: null,
        tools: next_tools,
        stop: null,
      }
    }
  }
}
```

`before_tool_call`, `after_tool_call`, and `on_event` use the default
`AgentHooks` implementations because this policy only changes the next roster.

`StepPlan.tools` is the complete next roster, so adding one tool means returning
the old tools plus the new one. The driver validates the replacement, updates
its registry, and sends the new schemas on the next provider turn. The task
declaration is not rewritten.

## Collision and capability rules

Tool names must be unique after provider-, task-, driver-, and registry-owned
tools are combined. An explicit replacement operation is required to replace a
name. Providers whose wire protocol fixes tools at `begin` must expose that
limitation; the safe driver rejects mutation instead of silently ignoring it.

## Related design


- [Dynamic tools](../specification/03-drivers.md#dynamic-tools)
