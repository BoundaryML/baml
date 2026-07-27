# Resume an Agent

Resumption is configured on the Agent runner, not stored as mutable state on
the task.

## Utilities used

| Utility | Purpose |
| --- | --- |
| `Agent.new(conversation = ...)` | Resumes exact provider state |
| `Done<T>.conversation` | Returns the final resumable conversation |
| `BudgetReached.conversation` | Returns state after a safe stop |

## Example

```baml
class Resolution {
  reply: string,
}

function lookup_order(order_id: string) -> string {
  "out for delivery"
}

function ResolveTicket(message: string) -> Resolution {
  provider: "openai/gpt-5.6-luna"
  prompt: `
    Continue resolving this support ticket.

    ${message}

    ${ctx.output_format}
  `
  tools: [lookup_order]
}

let resumed = ResolveTicket.task("Please continue.").run(
  runner = ai.run.Agent.new(
    conversation = saved_conversation,
    max_steps = 4,
  ),
)
```

The runner verifies that the task's selected provider matches the conversation
owner before sending a request. The task's declared return type and
application tools still describe the resumed run.

If the application wants another provider, it imports the portable messages
explicitly instead of relabeling the existing conversation.

[Back to conversations and state](../conversations-and-state.md)
