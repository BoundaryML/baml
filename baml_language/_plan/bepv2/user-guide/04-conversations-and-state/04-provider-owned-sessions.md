# Provider-owned sessions

Use a session when the provider stores conversation state and later calls must
execute in that exact context.

## Open and use a session

```baml
let session = ai.drivers.open_session(
  SessionModel,
  ai.SessionOptions {},
)
defer { session.cleanup() }

let first = ai.drivers.run_in_session(
  session,
  ContinueSupport.task(ai.Conversation.empty(), "Where is order 4821?"),
)

let second = ai.drivers.run_in_session(
  session,
  ContinueSupport.task(ai.Conversation.empty(), "Can I change its address?"),
)
```

The session context wins: the driver rebinds each task to
`session.provider()` and re-renders it there.

## Capability refinements

```baml
function explore(session: ai.ForkableSession) -> Resolution[] {
  let alternate = session.fork()
  [
    session.run(ResolveTicket.task(original)).value,
    alternate.run(ResolveTicket.task(alternative)).value,
  ]
}
```

Code that requires forking or provider-side compaction demands
`ForkableSession` or `CompactableSession`. A plain `Session` does not expose
methods that merely throw “unsupported.”

## Related design and scenarios

- [Sessions](../../pages/06-resources.md#sessions)
- Scenarios 17 history and sessions, 19 fork and branch, 20 server chains, 40 harness sessions

