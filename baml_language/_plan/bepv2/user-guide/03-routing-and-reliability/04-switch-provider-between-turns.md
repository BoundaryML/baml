# Switch provider between agent turns

Use `prepare_step` for a planned semantic handoff during an agent loop.

## Policy

```baml
class EscalateRefunds {
  implements ai.AgentHooks {
    function prepare_step(self, ctx: ai.StepContext) -> ai.StepPlan throws never {
      let next = if (ctx.step >= 3 || ctx.usage.cost_usd > 0.20) {
        CarefulToolModel
      } else {
        null
      }

      ai.StepPlan { provider: next, tools: null, stop: null }
    }

    function before_tool_call(self, event: ai.BeforeToolCall)
      -> ai.ToolDecision throws never {
      ai.ToolDecision.allow(event.call)
    }

    function after_tool_call(self, event: ai.AfterToolCall) -> void throws never {}
    function on_event(self, event: ai.AgentEvent) -> void throws never {}
  }
}
```

## Run it

```baml
let events = ai.drivers.stream_agent(
  ResolveTicket.task(ticket, $provider = FastToolModel),
  ai.AgentOptions {
    tools: [lookup_order, search_policy],
    hooks: EscalateRefunds {},
  },
)
```

## What the driver must do

```text
old Transcript
  -> provider-neutral Conversation
  -> target.import_conversation(...)
  -> TranscriptImport { transcript, fidelity, warnings }
  -> task.with_provider(target)
  -> ProviderChanged event
```

The target must implement `TranscriptImportProvider`. The driver never flattens
history into text as a silent fallback. Provider-private reasoning signatures,
caches, and remote continuation IDs may be lost and must be reported.

## Related design and scenarios

- [Provider switching](../../pages/03-drivers.md#provider-switching-halfway-through-a-loop)
- Scenarios 28 provider diversity, 30 routing, 39 extensibility
