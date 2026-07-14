# Observe an agent with events and hooks

Use events for UI, logs, traces, and metrics. Use hooks for decisions that may
change the next step. Neither is the provider's mutable transcript.

## Stream events

```baml
let events = ai.drivers.stream_agent(
  ResolveTicket.task(ticket, $provider = ToolModel),
  ai.AgentOptions { tools: [lookup_order, search_policy] },
)

while (true) {
  match (events.next()) {
    null => break,
    let event: ai.AgentEvent => match (event) {
      let e: ai.TextDelta => ui.append(e.text),
      let e: ai.ToolCallStarted => audit.tool_started(e.call),
      let e: ai.ToolCallFinished => audit.tool_finished(e.call, e.result),
      let e: ai.UsageUpdated => meter.add(e.usage),
      _ => {},
    },
  }
}
```

The stream includes model, reasoning-summary, tool, provider-change,
tool-roster, usage, and terminal events.

## Attach hooks

```baml
class SupportHooks {
  implements ai.AgentHooks {
    function prepare_step(self, ctx: ai.StepContext) -> ai.StepPlan throws never {
      ai.StepPlan { provider: null, tools: null, stop: null }
    }

    function before_tool_call(self, event: ai.BeforeToolCall)
      -> ai.ToolDecision throws never {
      ai.ToolDecision.allow(event.call)
    }

    function after_tool_call(self, event: ai.AfterToolCall) -> void throws never {}

    function on_event(self, event: ai.AgentEvent) -> void throws never {
      log.debug(event.kind())
    }
  }
}

let run = ai.drivers.run_agent(
  ResolveTicket.task(ticket, $provider = ToolModel),
  ai.AgentOptions {
    tools: [lookup_order],
    hooks: SupportHooks {},
  },
)
```

Provider metadata on reasoning or text blocks is observable. Exact signatures,
encrypted blocks, and continuation state remain provider-owned.

## Related design and scenarios

- [Running and observing the loop](../../pages/05-tools-and-agents.md#running-and-observing-the-loop)
- Scenarios 32 observability, 39 harness extensibility, 42 harness abstraction
