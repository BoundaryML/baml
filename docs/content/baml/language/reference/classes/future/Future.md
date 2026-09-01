---
title: "future.Future"
description: "Class future.Future from the generated baml package reference."
---

No description is available yet.

```baml
class future.Future<T, E>
```

## Methods

### cancel

```baml
function cancel(self: baml.future.Future<T, E>) -> bool
```

Request cancellation of the future. If the future is already
settled, this is a no-op and returns `false`. Otherwise the
future's cancel token fires; the spawned thread will throw
`baml.panics.Cancelled` at its next `await` point.

Returns `true` if a transition to the Cancelled state was
performed, `false` if the future was already settled.

### is_cancelled

```baml
function is_cancelled(self: baml.future.Future<T, E>) -> bool
```

`true` if the future was cancelled before producing a value.
Implies `is_settled() == true`.

### is_error

```baml
function is_error(self: baml.future.Future<T, E>) -> bool
```

`true` if the future settled with an error. Implies
`is_settled() == true`.

### is_result

```baml
function is_result(self: baml.future.Future<T, E>) -> bool
```

`true` if the future settled with a successful value. Implies
`is_settled() == true`.

### is_settled

```baml
function is_settled(self: baml.future.Future<T, E>) -> bool
```

`true` if the future has reached a terminal state (Ready, Error,
or Cancelled). Once `true`, this method's result never changes.

### state

```baml
function state(self: baml.future.Future<T, E>) -> baml.future.FutureState
```

No description is available yet.

_Source: `<builtin>/baml/ns_future/future.baml:1006`_
