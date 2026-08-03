# Serving

## Generated SDKs

`baml generate` produces typed clients for Python and TypeScript. Tasks
are plain function calls; sessions are handles:

```python
from baml_sdk import b

# task
trip = b.PlanTrip("2 weeks in Japan")            # -> Itinerary

# session
s = b.session.PlanTrip.create(request="2 weeks in Japan")
turn = s.run()
if turn.is_done:
    print(turn.result.destination)
else:
    print(turn.message)

snap = s.snapshot()                              # str
s2 = b.session.PlanTrip.resume(snap)
```

```typescript
import { b } from "./baml_sdk";

const s = await b.session.PlanTrip.create({ request: "2 weeks in Japan" });
const turn = await s.run();
```

## In-process sessions

The default. The session runs inside your process; the journal is in
memory; `snapshot()`/`resume()` move it across requests. Nothing to
deploy.

## Stateless sessions

For request/response servers, resume-per-request:

```python
def handle(req):
    s = b.session.PlanTrip.resume(req.snapshot)   # None -> fresh session
    s.send(req.message)
    turn = s.run()
    return {"reply": turn.message if not turn.is_done else turn.result,
            "snapshot": s.snapshot()}
```

You own storage. The snapshot is an opaque string.

## baml serve

`baml serve` hosts sessions directly and exposes them over HTTP. Sessions
are named instances backed by the configured journal store.

```
POST /v1/session/PlanTrip/{id}/messages     submit a message -> 202 + receipt
GET  /v1/session/PlanTrip/{id}/turns/{receipt}   read the outcome (blocks or polls)
GET  /v1/session/PlanTrip/{id}/events?from={seq} journal tail (SSE)
GET  /v1/session/PlanTrip/{id}/stream            token deltas (SSE, ephemeral)
POST /v1/session/PlanTrip/{id}/interrupt
POST /v1/session/PlanTrip/{id}/events            send a custom event
```

Submission is asynchronous: the POST returns a receipt once the message is
durably admitted; the reply is read with the receipt. A crashed caller
re-reads with the same receipt instead of re-submitting.

The generated SDKs speak this protocol with the same session interface:

```typescript
const s = await b.session.PlanTrip.connect("https://agents.internal", { id: `issue-${n}` });
s.events().on("tool_requested", (e) => ui.spinner(e.tool));   // journal tail
s.stream().on("delta", (d) => ui.append(d));                  // ephemeral lane
const receipt = await s.send("triage this");
const turn = await s.read(receipt);
```

## The wire protocol

Two channels, mirroring the journal design:

- **Journal tail** — durable events, replayable from any `seq`. Drive UI
  state from this channel: it is complete, ordered, and identical to what
  recovery and replay see.
- **Stream** — token deltas and transient progress. Not recorded, not
  replayable. Cosmetic.

A client that only consumes the journal tail renders a correct, if less
lively, UI. A client that only consumes the stream will miss events.
