# Switch provider after a model-turn failure

This is intentionally not presented as an existing standard hook. The BEP
specifies planned switching through `prepare_step`, but `AgentHooks` does not
yet define a failure decision.

## Desired application policy

```baml
// Proposed interface and hook, not yet normative.
class SwitchOnRateLimit {
  // ...fallback policy fields...

  implements ai.AgentFailureHooks {
    function on_model_failure(self, ctx: ai.ModelFailureContext)
      -> ai.FailureDecision throws never {
      if (ctx.failure.kind() == baml.errors.FailureKind.RateLimit) {
        return ai.FailureDecision.switch_to(CarefulToolModel)
      }
      ai.FailureDecision.stop()
    }

    // ...other AgentFailureHooks methods...
  }
}
```

## What a safe custom driver checks

```text
1. Classify the failure.
2. Ask the failed operation for ReplayPolicy.
3. Refuse if ai.may_replay returns false.
4. Refuse after observable output that cannot be retracted.
5. Refuse after an unkeyed or unknown-commit side effect.
6. Export the current Conversation.
7. Import it into the target provider and report fidelity.
8. Re-render the task and continue with a ProviderChanged event.
```

Until a standard failure hook is normative, applications should either wrap a
bounded replay-safe operation with `Fallback` or write a custom driver that
implements all eight steps. Catching an arbitrary error and changing a field
is not sufficient.

## Important distinction

```text
planned switch: prepare_step before the next turn
failure switch: replay decision after a failed turn
```

The latter needs failure, commit, transcript, and observed-output context that
ordinary `prepare_step` does not currently receive.

## Related design and scenarios

- [Failure and replay model](../../pages/08-reliability-and-errors.md)
- This page records an open contract needed by the user guide.
