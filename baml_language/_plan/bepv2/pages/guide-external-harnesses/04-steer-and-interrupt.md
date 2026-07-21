# Steer and interrupt a harness

> **Status:** Implemented in the executable reference.

Long-lived harnesses may support control verbs beyond ordinary tool results:
follow up, steer, interrupt, change model, compact, or rewind files.

## Send controls to an owned session

```baml
CodeHarness.steer(session, "Focus on the billing implementation.")
CodeHarness.interrupt(session)
let token = CodeHarness.save_session(session)
```

Steering appends guidance to the current run. Interruption asks the harness to
stop current work while keeping resumable state. `session.cleanup()` releases the
session permanently. These must remain distinct operations.

## Provider-specific extensions

The stable `Harness` contract exposes `steer` and `interrupt`. An adapter with
additional verbs such as file rewind should expose a refinement interface and
applications should narrow to it explicitly. Loss of portability then remains
visible at the narrowing site rather than hiding in an untyped command string.
