# Poll, cancel, and resume a job

A background job may be observed from another process.

## Utilities used

| Utility | Purpose |
| --- | --- |
| `Job.poll()` | Returns the completed response when ready |
| `Job.cancel()` | Requests cancellation |
| `Job.token()` | Produces serializable resume coordinates |

## Example

```baml
class Resolution {
  reply: string,
}

function DeepResolveTicket(message: string) -> Resolution {
  provider: BackgroundModel
  prompt: `
    Investigate and resolve this support ticket.

    ${message}

    ${ctx.output_format}
  `
}

function resolve_ticket_in_background(
  message: string,
  poll_delay: baml.time.Duration,
) -> Resolution {
  let job = DeepResolveTicket.task(message).run(
    runner = ai.run.Background.new(
      idempotency_key = "ticket-1042:deep-resolution",
    ),
  );

  defer { job.close() }

  while (job.status() == ai.JobStatus.Pending) {
    match (job.poll()) {
      let response: ai.Response<Resolution> => return response.value,
      null => baml.sys.sleep(poll_delay),
    }
  }

  baml.sys.panic("background job ended without a resolution")
}
```

Persist `job.token()` when another worker will resume polling. The token uses
stable provider ownership and versioning; it does not serialize application
credentials.

`cancel()` and `close()` are idempotent. `cleanup()` applies the resource's
documented abandoned-handle policy if garbage collection gets there first.
Closing a local handle cancels remote work only when that job contract says it
does.

[Back to production resources](../production-resources.md)
