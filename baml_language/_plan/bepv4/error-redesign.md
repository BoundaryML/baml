# Archived alternative: closed `ai.Error`

This document previously proposed replacing the open `ai.Failure` interface
with one closed `ai.Error` class. Compiler probes and the requirement that
applications and provider packages define their own statically catchable
failure types rejected that design.

The accepted proposal is
[the `Failure | UnknownError` plan](error-model-plan.md). The reader-facing
contract is documented in
[Errors and error handling](pages/errors-and-error-handling.md).

The useful conclusions retained from this alternative are:

- errors carry facts; recovery policy decides what to do;
- parse failures retain raw output and response context;
- ordinary state-machine termination is a value (`Done`, `BudgetReached`, or
  `Handoff`), while failure to fulfill the task contract is an error;
- retry exhaustion and fallback exhaustion preserve the real classified
  failure;
- effect safety is required before replay.

The Agent-only lifecycle sharpens the final point. Reliability wrappers operate
at the provider-step boundary:

```text
AgentProvider.begin  → create local provider conversation state
AgentProvider.step   → make exactly one model request
AgentProvider.submit → record correlated application-tool results
```

Retry can repeat a safe `step`; it never restarts `ai.run.Agent`. Fallback can
select another provider only before a successful model turn. Once progress has
occurred, an intentional provider change requires portable message export and
explicit import.

No code should be implemented from the former closed-error sketches. They are
superseded in full.
