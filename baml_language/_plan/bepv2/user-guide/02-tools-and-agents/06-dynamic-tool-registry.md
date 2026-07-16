# Change the tool registry between turns

A long-running agent does not need to expose every possible tool up front. A
driver-owned `ToolRegistry` may grow or shrink between provider turns.

## Start with a small roster

```baml
let registry = ai.ToolRegistry.new([lookup_order])

let options = ai.AgentOptions {
  ...default_agent_options(),
  tools: [],
  tool_registry: registry,
}

let run = ai.drivers.run_agent(
  ResolveTicket.task(ticket, $provider = ToolModel),
  options,
)
```

`AgentOptions` is configuration, so construct the complete value once. The
registry itself is intentionally stateful and is the object hooks mutate
between steps.

## Update the next step

```baml
class DynamicToolHooks {
  // ...policy fields and helper methods...

  implements ai.AgentHooks {
    function prepare_step(self, ctx: ai.StepContext) -> ai.StepPlan
        throws baml.errors.ToolError {
      if (ctx.step == 2 && refund_permission_granted(ctx)) {
        ctx.tool_registry.add(issue_refund)
      }

      ai.StepPlan {
        provider: null,
        tools: null,
        stop: null,
      }
    }

    // ...other AgentHooks methods may be omitted because they have defaults...
  }
}
```

The next provider turn receives the new schemas. The task declaration is not
rewritten.

## Collision and capability rules

Tool names must be unique after provider-, task-, driver-, and registry-owned
tools are combined. An explicit replacement operation is required to replace a
name. Providers whose wire protocol fixes tools at `begin` must expose that
limitation; the safe driver rejects mutation instead of silently ignoring it.

## Related design and scenarios

- [Dynamic tools](../../pages/03-drivers.md#dynamic-tools)
- Scenario 13 searchable tools
