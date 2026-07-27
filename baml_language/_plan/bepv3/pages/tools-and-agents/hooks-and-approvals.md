# Hooks and approvals

Hooks make execution decisions. Observers only watch.

## Utilities used

| Utility | Purpose |
| --- | --- |
| `ai.AgentHooks` | Supplies explicit execution decisions |
| `ai.ToolDecision` | Allows, replaces, or blocks a call |
| `ai.StepPlan` | Changes the next provider step |

## Example

```baml
class Resolution {
  reply: string,
}

function issue_refund(order_id: string, amount_usd: float) -> string {
  `refund scheduled for ${order_id}`
}

function ResolveTicket(message: string) -> Resolution {
  provider: "openai/gpt-5.6-luna"
  prompt: `
    Resolve this support ticket.

    ${message}

    ${ctx.output_format}
  `
  tools: [issue_refund]
}

class RefundApproval {
  approved: bool,

  implements ai.AgentHooks {
    function before_tool_call(
      self,
      event: ai.BeforeToolCall,
    ) -> ai.ToolDecision {
      if (event.call.name == "issue_refund" && !self.approved) {
        ai.ToolDecision.block("human approval required")
      } else {
        ai.ToolDecision.allow(event.call)
      }
    }
  }
}

let outcome = ResolveTicket.task("Refund order order-42.").run(
  runner = ai.run.Agent.new(
    hooks = RefundApproval { approved: false },
  ),
)
```

A blocked call still receives one correlated tool result. The provider may
explain the denial, choose another tool, or finish without the effect.

Approval lives in application policy rather than prompt text. Prompt
instructions may guide a model, but they are not an authorization boundary.

[Back to tools and agents](../tools-and-agents.md)
