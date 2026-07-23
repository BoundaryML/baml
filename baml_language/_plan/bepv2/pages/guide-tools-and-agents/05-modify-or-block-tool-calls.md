# Modify or block a tool call

> **Status:** Implemented in the executable reference.

Telemetry observes immutable events. Tool middleware owns behavioral policy
and may replace a proposed call, deny it, or replace its result.

## Block an effectful call

```baml
class RefundApproval {
  approved: bool,
  customer_id: string,

  implements ai.AgentHooks {
    function prepare_step(self, ctx: ai.StepContext) -> ai.StepPlan throws never {
      ai.StepPlan { provider: null, tools: null, stop: null }
    }

    function before_tool_call(
      self,
      event: ai.BeforeToolCall,
    ) -> ai.ToolDecision throws never {
      if (event.call.name == "issue_refund" && !self.approved) {
        return ai.ToolDecision.block("human approval required")
      }
      ai.ToolDecision.allow(event.call)
    }

    function after_tool_call(self, event: ai.AfterToolCall) -> null throws never {}
    function on_event(self, event: ai.AgentEvent) -> null throws never {}
  }
}
```

Attach the hook to the explicit agent run:

```baml
let run = ai.drivers.run_agent(
  ResolveTicket.task(ticket, $provider = ToolModel),
  ai.AgentOptions.new(
    tools = [lookup_order, issue_refund],
    hooks = RefundApproval {
      approved: false,
      customer_id: ticket.customer_id,
    },
  ),
)
```

The denied call becomes a correlated tool result. The model can explain the
denial, choose another action, or request a handoff.

## Rewrite arguments

```baml
class CustomerScopedTools {
  customer_id: string,

  implements ai.AgentHooks {
    function prepare_step(self, ctx: ai.StepContext) -> ai.StepPlan throws never {
      ai.StepPlan { provider: null, tools: null, stop: null }
    }

    function before_tool_call(self, event: ai.BeforeToolCall)
      -> ai.ToolDecision throws never {
      if (event.call.name == "lookup_order") {
        return ai.ToolDecision.allow(
          enforce_customer_id(event.call, self.customer_id),
        )
      }
      ai.ToolDecision.allow(event.call)
    }
  }
}
```

`after_tool_call` and `on_event` use the interface defaults. Here,
`enforce_customer_id` is an application helper that parses the call as
`LookupOrderArgs`, replaces `customer_id`, and returns a new `ToolCall` with
the original `id`.

Preserve the original call ID so the provider can correlate the result. Parse
and validate rewritten arguments before dispatch; middleware must not create a
schema-invalid call.

`after_tool_call` observes the completed call. Replacing a model-visible result
requires `ToolMiddleware`; its public attachment spelling is present in
`TaskOptions` but still needs a complete normative example before this guide
should recommend it.
