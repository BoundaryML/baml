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

## Enum arguments

A BAML enum is generated as a `(str, enum.Enum)` class. Where a BAML function
declares an enum, a call can pass a member of that class or a plain `str` that
equals the name of a variant. A host often has only the string: a name read
from JSON, from a flag or from a configuration file.

```python
Resolve(HostClientName.BedrockSonnet5)
Resolve("BedrockSonnet5")  # the same variant
```

The string becomes the variant, also inside a list, a map or a class field.
A string that names no variant raises `TypeError`, and the message lists the
variants. Where the declared type also takes a string as it is
(`string | HostClientName`, or a string literal that equals the value), the
string stays a string.

The generated annotations still name the enum, so a type checker expects a
member. A callback that is declared to return an enum must return a member:
the return value of a callback is not an argument.

## Requirements

- Python 3.10+
- `protobuf >= 6.31.1`

## License

Apache-2.0
