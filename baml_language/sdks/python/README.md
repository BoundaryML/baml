# baml_bridge

Python bindings for the BAML runtime (powered by `bex_engine`).

`baml_bridge` is the bridge layer that generated `baml_sdk` packages
import at runtime: it provides the `BamlRuntime` singleton, the
protobuf encoder/decoder and the function/method factories.

```python
from baml_bridge import BamlRuntime

rt = BamlRuntime.initialize_runtime(
    root_path=".",
    files={"main.baml": baml_source},
)
```

This package is generally consumed indirectly via the code generated
by `baml-cli generate --target python` — direct use is reserved for
runtime authors and bridge tests.

## Python callback execution

Passing a Python callable to BAML uses the environment of the call that
invokes it. Reusing a callback across threads, event loops, or requests does
not retain its registration environment.

| BAML entry | Python callback | Execution environment |
| --- | --- | --- |
| Synchronous | Synchronous | Calling Python thread, with its current context |
| Synchronous | Coroutine result | Bridge-owned worker loop, with copied caller context |
| Asynchronous | Synchronous or coroutine result | Originating application loop, with copied entry context |

The generated type of a callback parameter or class field accepts both forms:
`typing.Callable[[...], typing.Union[R, typing.Coroutine[typing.Any, typing.Any, R]]]`,
so a type checker takes a plain function and an `async def` for it.

Each dispatch of an async entry receives a separate copy of its application
`ContextVars`. Callback writes do not change the awaiting task's context or
another dispatch's context. An async callback need not run in the caller's
exact `asyncio.Task`.

A synchronous entry blocks its calling thread, including any application loop
running there. Coroutine callbacks of that entry must use resources of their
executing worker loop. Awaiting resources owned by the blocked loop is unsupported.

Returned BAML closures support `closure(...)` and
`await closure.call_async(...)`. Passing a closure back into BAML preserves
its native function handle. Each new invocation selects its own environment.

Cancelling a BAML call cancels executing coroutine callbacks on their owning
loops. Callback tasks retain their arguments and context until their bodies
and async cleanup finish. Cancellation does not interrupt a synchronous Python
body. A callback that suppresses cancellation may finish later; its result
does not revive the cancelled BAML call.

## Unobserved errors of spawned tasks

A BAML `spawn { ... }` task can end with an error that no code awaits. The
runtime finds such a task at a later garbage collection or at shutdown, and
reports its error one time to one handler for the process. The default handler
prints the traceback and ends the process, as an uncaught exception ends a
script (exit status 1; on Windows the process terminates itself with
`SIGTERM`). If the task was cancelled, the default only prints.

A server must usually not end because of one stray task. It installs a
handler that reports the error and returns:

```python
import logging

import baml_bridge


def report(error: BaseException, cancelled: bool) -> None:
    logging.getLogger("baml").error("unobserved BAML spawn error", exc_info=error)


baml_bridge.set_unhandled_spawn_error_handler(report)
```

- `error` is the exception that a call raises for the same failure (a
  `BamlError`, a `BamlPanic`, or the exception a Python callback raised). Its
  traceback has the BAML frames. `cancelled` is `True` when the task was
  cancelled: its cancellation fired before it failed, or `cancel()` was called
  on it at any time, also after it failed.
- The handler runs on a thread of the BAML runtime, not on the thread or the
  event loop that made the call, and no event loop runs on that thread. It must
  return soon and must not call BAML. To reach the application, use
  `loop.call_soon_threadsafe` or a queue.
- If the handler raises, the bridge prints the error of the task, gives the
  exception of the handler to `sys.unraisablehook`, and the process continues.
- `set_unhandled_spawn_error_handler` returns the handler that it replaces.
  `None` restores `default_unhandled_spawn_error_handler`. A handler can call
  the default for the errors that must end the process.

## Thrown values with no Python class

A `BamlError` or `BamlPanic` carries the thrown BAML value as `.value`. Where
the generated SDK has a class for it, `.value` is an instance of that class.
A thrown value can also be of a class that has no Python class: a process that
uses `baml_bridge` with no generated SDK has none, and a program can make a
class at run time. Such an instance is a `baml_bridge.BamlClassInstance`:

```python
try:
    call_function_sync(runtime, "missing", {}).result()
except BamlError as error:
    error.class_name                # "baml.errors.InvalidArgument"
    error.value.class_name          # the same name, also on a nested instance
    error.value.fields["message"]   # fields under their BAML names
```

A returned value is different: it has a declared type that a typed caller
relies on, so a returned instance of a class with no Python class is an error.

## Requirements

- Python 3.10+
- `protobuf >= 6.31.1`

## License

Apache-2.0
