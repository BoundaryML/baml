# Submit a batch

> **Status:** Partial — the resource contract and deterministic provider are
> implemented. A live provider batch adapter is still future work.

Batching submits several tasks of the same result type as one provider-owned
operation. Use it when the provider has a real batch API and immediate results
are not required.

## Submit related tasks

```baml
let tasks = tickets.map((ticket) -> {
  ResolveTicket.task(ticket, $provider = BatchModel)
})

let batch = ai.drivers.submit_batch(
  BatchModel,
  tasks,
  ai.BatchOptions { idempotency_key: upload_id },
)
defer { batch.cleanup() }
```

The explicit provider owns the batch. All tasks return the same `T`, so
`results()` can return `Response<T>[]` without erasing types.

## Read status and results

```baml
match (batch.status()) {
  ai.JobStatus.Complete => {
    let responses = batch.results()
    persist(responses.map((response) -> { response.value }))
  },
  ai.JobStatus.Pending => schedule_another_poll(),
  ai.JobStatus.Failed => report_batch_failure(),
  ai.JobStatus.Cancelled => report_cancellation(),
}
```

`cancel()` asks the provider to stop unfinished work. `cleanup()` releases the
resource and is idempotent. A provider adapter must document whether cleanup
also cancels remote work; callers should use `cancel()` when cancellation is
their intent.

## What to test

Use a deterministic batch provider to verify that results keep their input
order and type, cancellation changes the status, and repeated cleanup has no
additional effect. A provider adapter should add a separate live test for its
remote batch API.

See [Resources](../specification/07-resources.md) for the lifecycle contract.
