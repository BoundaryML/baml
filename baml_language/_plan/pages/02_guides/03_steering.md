# Steering

## The two lanes

A running session accepts input on two lanes:

- **Data lane** — `send()`. Queued. The policy decides when the model
  sees it.
- **Control lane** — `interrupt()`. Immediate. Running tools and
  subagents are cancelled now.

The lanes exist because a cancel that waits in line behind queued
messages is not a cancel.

## Queued messages

`send()` while the agent is mid-run does not disturb it. Messages queue,
and the default policy injects them at the next turn boundary — after the
current model call and its tool batch complete:

```baml
s.send("also check Kyoto");        // agent is running; queues
s.send("budget is $3000 total");   // queues behind the first
// both are injected before the next model call
```

The journal records the injection, not the queueing: `UserMessage` events
appear at the point the model actually saw them. What the model saw and
when is always reconstructible.

Injection timing is policy behavior. The default is turn-boundary
injection; a custom policy can hold messages longer (finish the current
plan step first) or flush them earlier. See the steering middleware in
`../03_examples/01_claude_code.md` for a complete implementation.

## Interrupts

```baml
s.interrupt("user pressed esc");
```

`interrupt`:

1. Cancels in-flight tool calls and child sessions through their cancel
   tokens. Cancellation is cooperative and flows down the session tree.
2. Appends an `Interrupted` event once it takes effect.
3. Hands the event to the policy. The default policy calls the model so
   it can react to the interruption.

## Emitting events from outside

Applications append custom events to a running session with `emit`:

```baml
s.emit(PermissionGranted { call_id: "t7" });
```

`emit` is typed by the session's event union (`10_journal.md`). Use it
for approvals and external signals — input that the policy should react
to but that is neither a user message nor an interrupt.
