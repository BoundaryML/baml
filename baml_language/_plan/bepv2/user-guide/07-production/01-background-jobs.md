# Submit a background job

Background work is a remote resource, not an in-process future. It can outlive
the caller and be resumed by another process.

## Submit

```baml
let job = ai.drivers.submit_background(
  DeepResolveTicket.task(ticket, $provider = BackgroundModel),
  ai.BackgroundOptions {
    idempotency_key: `ticket:${ticket.id}:deep-resolution`,
  },
)
defer { job.cleanup() }
```

## Poll

```baml
while (true) {
  match (job.poll()) {
    let done: ai.Done<Resolution> => return done.value,
    let pending: ai.Pending => baml.sys.sleep(pending.retry_after),
    let failed: ai.Failed => throw failed.error,
    let cancelled: ai.Cancelled => return queue_for_review(ticket),
  }
}
```

## Cross a process boundary

```baml
db.save(ticket.id, baml.json.to_string(job.token()))

// Later, on a configured provider:
let token = baml.json.from_string<ai.JobToken>(db.load(ticket.id))
let resumed = BackgroundModel.resume_job<Resolution>(token)
```

Application tools require an application worker and cannot be smuggled into a
provider-owned background job. Use an external harness for a long-lived tool
loop.

## Related design and scenarios

- [Background jobs](../../pages/06-resources.md#background-jobs)
- Scenario 27 background jobs

