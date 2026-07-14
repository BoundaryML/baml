# Call a provider capability directly

Drivers are the normal application surface. Direct capability calls are the
supported escape hatch for custom lifecycle policy.

## Use the standard one-turn driver

```baml
let task = ResolveTicket.task(ticket, $provider = FastModel)
let response = ai.drivers.generate_with_meta(task)
```

## Go one level lower

When the concrete provider is statically known, call its capability method:

```baml
let task = ResolveTicket.task(ticket, $provider = FastModel)
let response = FastModel.generate<Resolution>(task)
```

The provider still owns request rendering, authentication, wire parsing, and
typed output validation. Application code should not reproduce those pieces
with raw `fetch` calls.

## Write a custom driver

```baml
function generate_and_audit<T, P extends ai.GenerationProvider>(
  task: ai.Task<T, P>,
) -> ai.Response<T> {
  let response = task.$provider.generate<T>(task)
  audit.record(task.identity, response.meta)
  response
}

let response = generate_and_audit(ResolveTicket.task(ticket))
```

A custom driver is an ordinary generic BAML function. It needs no compiler
plugin and does not create another generated member on every LLM function.

## Function values do not retain `.task`

```baml
let callable = ResolveTicket
let value = callable(ticket) // valid

// Compile error: `.task` is resolved only on the declaration path.
// let task = callable.task(ticket)
```

Pass the task itself when another function needs deferred execution.

## Related design and scenarios

- [Custom drivers](../../pages/03-drivers.md#custom-drivers)
- [Custom capabilities](../../pages/07-custom-capabilities.md)
- Scenario families: 03 constrained decoding, 36 capability negotiation

