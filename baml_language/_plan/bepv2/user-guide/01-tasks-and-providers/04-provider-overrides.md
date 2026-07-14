# Override or rebind a provider

The declaration supplies a default. Callers can select a different provider
without creating another LLM function.

## Override a direct call

```baml
let resolution = ResolveTicket(
  ticket,
  $provider = CarefulModel,
)
```

Because the direct form promises `Resolution`, `CarefulModel` must implement
`DriveProvider`.

## Override task construction

```baml
let task = ResolveTicket.task(
  ticket,
  $provider = ToolModel,
)

let run = ai.drivers.run_agent(task)
```

The task form accepts any `Provider`; the selected driver supplies the narrower
capability requirement.

## Rebind an existing task

```baml
let cheap = ResolveTicket.task(ticket, $provider = FastModel)
let careful = cheap.with_provider(CarefulModel)
```

`with_provider` re-renders the task from its private render recipe. It does not
just replace a field: output-format instructions, media encoding, tool schema,
and provider-specific prompt context may change.

## Static and runtime routing

Prefer a signature that states the capability:

```baml
function resolve_with(
  ticket: Ticket,
  provider: ai.DriveProvider,
) -> Resolution {
  ResolveTicket(ticket, $provider = provider)
}
```

If routing intentionally returns only `Provider`, capability evidence is gone:

```baml
let selected: ai.Provider = route_for(tenant)
let task = ResolveTicket.task(ticket, $provider = selected)
let result = ai.drivers.unsafe.drive(task)
```

The unsafe spelling performs a typed runtime capability check; it does not
disable parsing or output validation.

## Related design and scenarios

- [Providers and capabilities](../../pages/04-providers-and-capabilities.md)
- Scenario families: 28 provider diversity, 30 cascades and routing, 36 capability negotiation

