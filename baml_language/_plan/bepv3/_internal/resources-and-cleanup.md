# Resources and cleanup

Streams, jobs, batches, live sessions, MCP servers, and external harnesses are
resources. They may own sockets, remote work, files, tasks, or provider state.

## Resource protocol

Every resource has:

- an explicit idempotent `close()` or domain-specific terminal operation;
- a `cleanup()` fallback that runs when the value becomes unreachable;
- a stable trace identity;
- a clear terminal state; and
- no use-after-close behavior.

`cleanup()` is a special function recognized by the runtime. It runs when
garbage collection reclaims the value. It is a safety net, not the preferred
way to time important effects.

## Deterministic cleanup

Use `defer` when cleanup timing matters:

```baml
let session = ai.open_live(
  VoiceSupport.task(customer_id),
  channel,
)
defer { session.close() }

session.send_audio(audio)
```

Explicit methods should communicate domain intent:

- `stream.close()`;
- `job.cancel()` or `job.close()`;
- `batch.cancel()` or `batch.close()`;
- `session.close()`;
- `server.close()`; and
- `harness.close()`.

Calling a terminal method more than once must be safe.

## Cleanup fallback

A resource implementation may define:

```baml
function cleanup(self) {
  self.close()
}
```

Cleanup must:

- never resurrect the object;
- avoid blocking indefinitely;
- tolerate a partially initialized resource;
- be idempotent with explicit close;
- retain enough provider identity to release the right resource; and
- record failures without throwing into unrelated application code.

## Garbage collection tests

The runtime should expose a test-only or diagnostic GC trigger:

```baml
baml.runtime.collect_garbage()
```

The exact public namespace remains an implementation decision. The operation
must complete a full collection and run eligible cleanup functions before it
returns when used by deterministic tests.

Tests should prove:

1. unreachable resource cleanup runs;
2. reachable resource cleanup does not run;
3. explicit close followed by GC closes only once;
4. cyclic unreachable resources are collected;
5. partially constructed resources clean up safely; and
6. cleanup failure does not abort the collector.

Production applications should not call GC to coordinate lifecycle.

## Remote terminal states

Closing a local handle does not always cancel remote work. Each resource API
must state which operation it performs:

| Operation | Local handle | Remote work |
| --- | --- | --- |
| `close` | Releases | Provider-specific |
| `cancel` | Remains readable until terminal | Requests cancellation |
| `detach` | Releases | Continues |
| `wait` | Remains | Continues to completion |

There should be no generic `close` behavior that quietly promises remote
cancellation across all providers.
