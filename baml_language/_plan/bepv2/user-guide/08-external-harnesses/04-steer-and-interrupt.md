# Steer and interrupt a harness

Long-lived harnesses may support control verbs beyond ordinary tool results:
follow up, steer, interrupt, change model, compact, or rewind files.

## Negotiate before sending

```baml
function can_rewind(provider: ai.Provider) -> bool {
  match (provider) {
    let controls: ai.HarnessControlPlane => {
      controls.supported_verbs().includes("rewind_files")
    },
    _ => false,
  }
}
```

## Send a supported command

```baml
if (can_rewind(CodeHarness)) {
  session.send(ai.HarnessCommand {
    verb: "rewind_files",
    payload: { "checkpoint": checkpoint_id },
  })
}
```

Steering appends guidance to the current run. Interruption asks the harness to
stop current work while keeping resumable state. Destroying releases the
session permanently. These must remain distinct operations.

## Provider-specific extensions

Applications may narrow `agent.raw()` to a provider-specific control interface
for a verb not shared across harnesses. Loss of portability is explicit at the
narrowing site.

Exact `HarnessCommand` names are reference-level in the current scenarios;
the stable contract is capability negotiation plus typed session ownership.

## Related design and scenarios

- Scenarios 37 harness basics, 39 extensibility, 42 abstraction

