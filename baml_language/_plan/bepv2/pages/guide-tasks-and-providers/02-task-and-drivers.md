# Turn a call into `Task<T, P>`

> **Status:** Implemented in the executable reference.

A direct call chooses the provider's default lifecycle. Create a task value
when application code should choose the lifecycle explicitly.

## Use it

```baml
let task = ResolveTicket.task(ticket)
let resolution = ai.drivers.drive(task)
```

## What changed

```diff
- let resolution = ResolveTicket(ticket)
+ let task = ResolveTicket.task(ticket)
+ let resolution = ai.drivers.drive(task)
```

The result is the same. The second form exposes a typed value that can be
inspected, rebound, passed to another function, or consumed by a different
driver before any I/O happens.

## Inspect without executing

```baml
let task = ResolveTicket.task(ticket)
let prompt = ai.inspect.prompt(task)
let messages = ai.inspect.messages(task)
```

Inspection renders provider-aware messages but performs no network request.

## Why the provider type remains on the task

```text
ResolveTicket.task(ticket) -> Task<Resolution, SupportModelType>
```

The second type argument is capability evidence. A safe driver accepts the
task only when that concrete provider implements the required interface.

```baml
function run<T, P extends ai.DriveProvider>(task: ai.Task<T, P>) -> T {
  ai.drivers.drive(task)
}
```

Erasing the provider to `Task<Resolution>` also erases that proof. Use an
`ai.drivers.unsafe.*` function only when routing genuinely happens at runtime.

## Ownership

```text
compiler: declaration invocation -> Task<T, P>
driver:   Task<T, P> -> execution lifecycle
provider: capability call -> vendor protocol
```

## Related design


- [Tasks and philosophy](../specification/01-tasks-and-philosophy.md)
- [Drivers](../specification/03-drivers.md)
