# Fork a conversation

Application-owned conversations are values. Copy a prefix and continue several
branches independently.

## Fan out branches

```baml
function continue_branch(
  prefix: ai.Conversation,
  instruction: string,
) -> Resolution {
  ContinueSupport(prefix, instruction)
}

let conservative = spawn {
  continue_branch(history, "Offer the least disruptive resolution.")
}
let generous = spawn {
  continue_branch(history, "Offer the most customer-friendly resolution.")
}

let candidates = [await conservative, await generous]
let winner = ChooseResolution(ticket, candidates)
```

The original history is unchanged. This is ordinary structured concurrency;
there is no provider-owned branch unless a session resource is involved.

## Fork at an earlier point

```baml
let branch = history.slice(0, decision_turn)
  .append(ai.ConversationMessage.user("Try a different approach."))
```

## Provider-owned alternative

If exact remote state matters, require `ForkableSession` and call
`session.fork()`. Not every session provider supports it, so the capability is
explicit.

## Related design and scenarios

- [Session refinements](../../pages/06-resources.md#sessions)
- Scenario 19 fork and branch

