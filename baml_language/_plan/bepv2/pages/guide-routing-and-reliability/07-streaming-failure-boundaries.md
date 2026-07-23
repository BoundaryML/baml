# Streaming failure boundaries

> **Status:** Implemented in the executable reference.

Streaming introduces an observation boundary. Before the first chunk, the
operation may still be retried or moved. After a chunk is visible, automatic
restart would lie to the consumer.

## Safe initiation retry

```baml
let stream = ai.drivers.stream(
  ResolveTicket.task(ticket, $provider = FastModel.with_retry(policy)),
)
```

The wrapper may retry connection establishment or a classified failure before
the first emitted partial.

## After output begins

```text
chunk 1 observed
chunk 2 observed
transport fails
```

The stream surfaces `StreamError`. It does not silently call another provider
and emit a second version of chunk 1.

Application policy may:

- show a partial-result error;
- explicitly discard visible output and start a new stream;
- retain the partial transcript and ask the same provider to resume, if the
  provider has an explicit resumption capability; or
- switch providers through an explicit conversation import and tell the user
  continuity is lossy.

Realtime is stricter still: played audio, barge-in, and socket state are never
automatically replayed.

## Related design


- [Retry](../specification/09-reliability-and-errors.md#retry)
