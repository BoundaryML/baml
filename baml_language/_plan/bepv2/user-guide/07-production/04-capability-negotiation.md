# Negotiate capabilities intentionally

Prefer static capability requirements. Use runtime negotiation only when the
provider type was intentionally erased by configuration or routing.

## Static requirement

```baml
function resolve_streaming<P extends ai.StreamingProvider>(
  provider: P,
  ticket: Ticket,
) -> Resolution {
  ai.drivers.stream(
    ResolveTicket.task(ticket, $provider = provider),
  ).final()
}
```

Unsupported providers fail at the call site during type checking.

## Runtime negotiation

```baml
function resolve_configured(provider: ai.Provider, ticket: Ticket) -> Resolution {
  let task = ResolveTicket.task(ticket, $provider = provider)

  match (provider) {
    let stream: ai.StreamingProvider => ai.drivers.stream(task.with_provider(stream)).final(),
    let drive: ai.DriveProvider => ai.drivers.drive(task.with_provider(drive)),
    _ => throw baml.errors.Unsupported {
      message: "configured provider cannot resolve a ticket",
    },
  }
}
```

## Graded support

Interfaces answer whether an interaction shape exists. Provider descriptors
may answer graded questions such as supported image formats, context limits,
or whether tools may change after `begin`. Do not create a new capability for
every vendor option.

## Related design and scenarios

- [Safe and unsafe drivers](../../pages/03-drivers.md#safe-drivers-and-unsafe-drivers)
- Scenario 36 capability negotiation

