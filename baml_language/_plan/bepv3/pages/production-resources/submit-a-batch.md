# Submit a batch

A batch consumes a collection of tasks as one provider operation.

## Utilities used

| Utility | Purpose |
| --- | --- |
| `ai.run.Batch<T>` | Submits homogeneous tasks together |
| `ai.Batch<T>` | Tracks remote batch state and results |
| `idempotency_key` | Deduplicates batch submission |

## Example

```baml
class Classification {
  category: string,
}

function ClassifyTicket(message: string) -> Classification {
  provider: BatchModel
  prompt: `
    Classify this support ticket.

    ${message}

    ${ctx.output_format}
  `
}

let tasks = [
  ClassifyTicket.task("I was charged twice."),
  ClassifyTicket.task("Where is my package?"),
  ClassifyTicket.task("How do I reset my password?"),
];

let batch = ai.run.Batch<Classification>.new(
  provider = BatchModel,
  idempotency_key = "daily-ticket-classification",
).run(tasks);

defer { batch.close() }
```

The simple batch API is homogeneous: every task produces `Classification`.
This avoids pretending that invariant arrays safely upcast unrelated task
types.

A heterogeneous queue may return one typed item handle per submitted task:

```baml
let queue = ai.BatchQueue.new(provider = BatchModel);
let classification_item = queue.add(ClassifyTicket.task(message));
let resolution_item = queue.add(ResolveTicket.task(ticket));
let batch = queue.execute();

let classification: Classification = classification_item.result();
let resolution: Resolution = resolution_item.result();
```

The queue never stores one erased `T[]`; each handle preserves its own result
type.

[Back to production resources](../production-resources.md)
