# Cleanup and defer

Resources use BAML's standard cleanup semantics.

## Utilities used

| Utility | Purpose |
| --- | --- |
| `close()` | Explicit, idempotent resource release |
| `defer` | Calls an explicit operation on every scope exit |
| `cleanup()` | Special function called during garbage collection |

## Example

```baml
class Resolution {
  reply: string,
}

function DeepResolveTicket(message: string) -> Resolution {
  provider: BackgroundModel
  prompt: `
    Resolve this support ticket.

    ${message}

    ${ctx.output_format}
  `
}

function run_ticket(message: string) -> Resolution {
  let job = DeepResolveTicket.task(message).run(
    runner = ai.run.Background.new(),
  );

  defer { job.close() }

  job.wait().value
}
```

`defer` runs on return, throw, and normal fall-through. It calls `close()` at a
predictable time. `cleanup()` is the fallback finalizer BAML invokes when
garbage collection reclaims an unreachable job. Both paths share an
idempotent release implementation.

Deterministic production code uses `defer` with the domain operation it wants:
`close()`, `cancel()`, or `delete()`. Garbage collection is a safety net for
abandoned resources, not a remote-resource scheduling API.

Tests for finalization need a deterministic way to request collection and
observe cleanup without depending on timing.

[Back to production resources](../production-resources.md)
