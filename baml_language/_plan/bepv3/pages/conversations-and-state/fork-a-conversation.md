# Fork a conversation

Portable history can branch into two independent tasks.

## Utilities used

| Utility | Purpose |
| --- | --- |
| `MessageHistory.append` | Returns a new history |
| `task.with_messages(...)` | Starts from portable messages |

## Example

```baml
class Answer {
  text: string,
}

function ContinueSupport(instruction: string) -> Answer {
  provider: "openai/gpt-5.6-luna"
  prompt: `
    Continue the support conversation.

    ${instruction}

    ${ctx.output_format}
  `
}

let base = ai.MessageHistory.empty()
  .append(ai.ChatMessage.user("My order is late."));

let refund_branch = base.append(
  ai.ChatMessage.user("Check whether I qualify for a refund."),
);

let delivery_branch = base.append(
  ai.ChatMessage.user("Estimate the new delivery date."),
);

let refund = ContinueSupport.task("Evaluate refund options.")
  .with_messages(refund_branch)
  .run(runner = ai.run.Completion.new());

let delivery = ContinueSupport.task("Estimate delivery.")
  .with_messages(delivery_branch)
  .run(runner = ai.run.Completion.new())
```

Forking portable history does not mutate the original. Forking an exact
provider conversation requires provider support or an export/import boundary.

[Back to conversations and state](../conversations-and-state.md)
