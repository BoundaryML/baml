# Submit a background job

> **Status:** Implemented in the executable reference.

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
    let response: ai.Response<Resolution> => return response.value,
    null => match (job.status()) {
      ai.JobStatus.Pending => baml.sys.sleep(poll_delay),
      ai.JobStatus.Cancelled => return queue_for_review(ticket),
      ai.JobStatus.Failed => return report_failure(ticket),
      ai.JobStatus.Complete => baml.sys.sleep(poll_delay),
    },
  }
}
```

`poll()` throws when the provider reports a failed response. The explicit
status remains useful for logging, cancellation, and recovery code.

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

## Related design


- [Background jobs](../specification/07-resources.md#background-jobs)
