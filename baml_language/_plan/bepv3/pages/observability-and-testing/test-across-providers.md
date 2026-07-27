# Test one task across providers

Task values make provider matrices ordinary typed application code.

## Utilities used

| Utility | Purpose |
| --- | --- |
| `task.with_provider(...)` | Rebinds one task |
| `CompletionWithMeta` | Captures comparable metadata |
| `spawn` | Runs independent providers concurrently |

## Example

```baml
class Classification {
  category: string,
}

function ClassifyTicket(message: string) -> Classification {
  provider: FastModel
  prompt: `
    Classify this support ticket.

    ${message}

    ${ctx.output_format}
  `
}

let base = ClassifyTicket.task("I was charged twice.");

let runs = [
  spawn {
    base.with_provider(FastModel).run(
      runner = ai.run.CompletionWithMeta.new(),
    )
  },
  spawn {
    base.with_provider(CarefulModel).run(
      runner = ai.run.CompletionWithMeta.new(),
    )
  },
];

let responses = await baml.future.all(runs)
```

Each provider receives a prompt rendered for its own protocol while preserving
the same arguments and `Classification` contract.

[Back to observability and testing](../observability-and-testing.md)
