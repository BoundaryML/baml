# Switch provider between turns

An Agent may switch providers between model steps. The new provider imports a
portable message view and reports the fidelity of that move.

## Utilities used

| Utility | Purpose |
| --- | --- |
| `AgentHooks.prepare_step` | Chooses the next provider |
| `ConversationImportProvider` | Imports portable messages |
| `ConversationFidelity` | Reports exact, message-only, or lossy import |

## Example

```baml
class Resolution {
  reply: string,
}

function lookup_order(order_id: string) -> string {
  "out for delivery"
}

function ResolveTicket(message: string) -> Resolution {
  provider: FastToolModel
  prompt: `
    Resolve this support ticket.

    ${message}

    ${ctx.output_format}
  `
  tools: [lookup_order]
}

class EscalateAfterTwoSteps {
  implements ai.AgentHooks {
    function prepare_step(
      self,
      ctx: ai.StepContext,
    ) -> ai.StepPlan {
      if (ctx.step >= 2) {
        ai.StepPlan.switch_provider(CarefulToolModel)
      } else {
        ai.StepPlan.keep()
      }
    }
  }
}

let outcome = ResolveTicket.task("Where is order-42?").run(
  runner = ai.run.Agent.new(
    hooks = EscalateAfterTwoSteps {},
  ),
)
```

The Agent never passes one provider's concrete `Conversation` to another
provider. It exports portable messages, asks the new provider to import them,
and records fidelity and warnings.

Provider display names are for logs. They are not ownership identity.

[Back to routing and reliability](../routing-and-reliability.md)
