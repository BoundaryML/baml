# Fall back between providers

Fallback changes provider after a classified failure only when replay remains
safe.

## Use it

```baml
let ResilientModel = FastModel.fallback_to(CarefulModel)

let resolution = ResolveTicket(ticket, $provider = ResilientModel)
```

## Attempt flow

```text
FastModel
  success                         -> return
  replay-safe transport failure  -> rebind task to CarefulModel
  invalid request or refusal      -> surface failure
  unsafe/unknown committed effect -> surface failure
```

Every attempt receives `task.with_provider(member)`, so provider-sensitive
messages and output instructions are rendered for that member.

## Do not use fallback for business escalation

“Try a stronger model if a judge scores this answer poorly” is a cascade or
router, not failure fallback. The first model succeeded; application policy
decided the value was insufficient.

## Stateful boundary

A provider-owned session does not move to the next fallback member halfway
through. Pick a session provider before opening the resource, or perform an
explicit transcript export/import with fidelity reporting.

## Related design and scenarios

- [Fallback](../../pages/08-reliability-and-errors.md#fallback)
- Scenarios 29 reliability, 30 cascades and routing

