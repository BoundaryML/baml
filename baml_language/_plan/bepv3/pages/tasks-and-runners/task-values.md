# Create and inspect a task

A task is useful when code needs to inspect, store, route, or configure an LLM
function call before it runs.

## Utilities used

| Utility | Purpose |
| --- | --- |
| `Name.task(...)` | Creates a task |
| `task.messages()` | Returns the rendered portable messages |
| `task.output_type()` | Returns the declared result type |
| `task.with_provider(...)` | Rebinds and re-renders for another provider |

## Example

```baml
class Ticket {
  id: string,
  message: string,
}

class Resolution {
  reply: string,
  resolved: bool,
}

function ResolveTicket(ticket: Ticket) -> Resolution {
  provider: "openai/gpt-5.6-luna"
  prompt: `
    Resolve this support ticket.

    ${ticket}

    ${ctx.output_format}
  `
}

let task = ResolveTicket.task(
  Ticket {
    id: "ticket-1042",
    message: "Where is order order-42?",
  },
);

log.info(task.messages());
log.info(task.output_type());

let response = task.run(
  runner = ai.run.CompletionWithMeta.new(),
)
```

Nothing happens when `task` is created or inspected. The first provider
operation happens inside `runner.run(task)`.

## Rules

- Task construction is pure with respect to network and application effects.
- Prompt rendering may depend on the selected provider, but it performs no I/O.
- Rebinding a provider re-renders the prompt for that provider.
- A task retains the LLM function identity for graphs, traces, and tests.
- An ordinary function value does not gain a dynamic `.task` method;
  `ResolveTicket.task(...)` is resolved from the LLM declaration.

[Back to tasks and runners](../tasks-and-runners.md)
