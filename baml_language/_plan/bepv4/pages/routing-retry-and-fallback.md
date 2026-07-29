# Routing, retry, and fallback

Routing chooses a provider before a run. Retry repeats one safe model step.
Fallback may choose another provider only before the first successful model
turn.

## Route before the call

Use ordinary BAML control flow when the application already knows which
provider to use:

```baml
function provider_for(ticket: SupportTicket) -> ai.AgentProvider {
  if (ticket.customer_tier == "pro") {
    careful_model()
  } else {
    fast_model()
  }
}

let ticket = sample_ticket();
let outcome = ResolveTicket@task(ticket)
  .with_provider(provider_for(ticket))
  .run(runner = ai.run.Agent<Resolution>.new())
```

The router returns the capability needed for normal execution. Provider
selection happens before `AgentProvider.begin`, so no conversation or model
work is discarded.

The runnable scenario is:

```console
baml run --from crates/baml_tests/baml_src_temp2 \
  ai_scenarios.route_before_the_call
```

## Retry is per step

```baml
let provider = ai.retry(
  fast_model(),
  3,
);

let outcome = ResolveTicket@task(sample_ticket())
  .with_provider(provider)
  .run(runner = ai.run.Agent<Resolution>.new())
```

`ai.retry` implements `AgentProvider` by delegating `begin` and `submit` and
wrapping `step`:

```text
Agent calls retry.step
  ├─ inner.step succeeds → return the ModelStep
  └─ inner.step fails
       ├─ classified as safe → repeat this step
       └─ not safe / attempts exhausted → rethrow the real failure
```

The wrapper never calls `Agent.run`. It cannot replay an application tool
because tools execute outside the provider between `step` and `submit`.

When the inner provider implements `ConversationAppendProvider`, `ai.retry`
also preserves exact fresh-message append by delegating to that same inner
conversation.

The retry predicate is deliberately private and conservative:

```text
failure.is_transient() && failure.effects() == Effects.None
```

`Effects.Unknown` and `Effects.Committed` are never blind-retried. There is no
provider-wide replay-policy knob.

This also imposes a transactional provider contract: if a replay-safe `step`
fails, it must leave its `Conversation` unchanged. A retry calls `step` again
with that same pre-attempt state; partially recorded response IDs, pending
calls, or assistant content would make the replay unsafe.

For example:

```text
step 1 → request charge lookup
Agent executes lookup once
submit lookup result
step 2 → rate limit (transient, Effects.None)
retry repeats step 2 only
```

This is the central safety rule. Retrying an entire typed call after the lookup
would replay every effect before the failed turn.

## Fallback is pre-progress only

```baml
let provider = ai.fallback([
  fast_model(),
  careful_model(),
]);

let outcome = ResolveTicket@task(sample_ticket())
  .with_provider(provider)
  .run(runner = ai.run.Agent<Resolution>.new())
```

Fallback may advance only when the initial `step` fails with a transient,
`Effects.None` failure before any successful model turn. It re-renders the
original task for that member and starts a fresh provider conversation.
Failures from the first member's `begin` are not intercepted. Terminal
failures and failures with `Effects.Unknown` or `Effects.Committed` are
re-thrown without trying another member.

Once `step` has succeeded, fallback is no longer valid:

- the provider may have returned encrypted reasoning or continuation IDs;
- application tools may have run;
- the next request depends on exact provider state.

A later failure is therefore rethrown. The wrapper does not silently restart
on another member.

Nested composition remains useful:

```baml
ai.fallback([
  ai.retry(fast_model(), 3),
  careful_model(),
])
```

This retries the first member's initial step before trying the second member.
After either member makes progress, the chosen member remains authoritative.
Appending a fresh user message to a fallback-owned conversation has the same
effect: the wrapper delegates to the active member's
`ConversationAppendProvider` and pins that member, even if no model step had
previously succeeded. A later member cannot reconstruct the appended exact
state.

## Switch after progress through import

An intentional mid-run provider change uses portable messages:

```baml
let next = careful_model();
let importer = match (next) {
  let provider: ai.ConversationImportProvider => provider,
  _ => throw baml.errors.Unsupported {
    message: "destination cannot import conversations",
  },
};

let imported = importer.import_messages<Resolution>(
  conversation.messages(),
);

let outcome = ResolveTicket@task(sample_ticket())
  .with_provider(next)
  .run(
    runner = ai.run.Agent<Resolution>.new(
      conversation = imported.conversation,
    ),
  )
```

`Agent.prepare_step` can perform the same protocol declaratively by returning a
new provider. The Agent exports the current conversation's portable messages,
requires the destination to implement `ConversationImportProvider`, imports
the messages, and emits a provider-changed event with fidelity and warnings.

Switching a provider label without importing state is invalid.

Runnable examples:

```console
baml run --from crates/baml_tests/baml_src_temp2 \
  ai_scenarios.switch_provider_between_turns

baml run --from crates/baml_tests/baml_src_temp2 \
  ai_scenarios.switch_provider_after_failure
```

## Failure behavior

Reliability wrappers preserve the real error:

- retry exhaustion rethrows the last inner failure;
- fallback exhaustion rethrows the last member's failure;
- an unsafe later failure is not replaced by a wrapper-specific summary.

Classification therefore remains available to the caller. See
[Errors and error handling](errors-and-error-handling.md).

## Rules

| Situation | Allowed behavior |
| --- | --- |
| Provider selected before `begin` | Route freely |
| Initial `step` fails safely | Retry that step or fall back |
| Any provider `step` succeeded | Keep the selected provider |
| Application tool executed | Never replay it through provider retry |
| Need a different provider after progress | Export messages and explicitly import |
| Retry/fallback exhausted | Rethrow the real classified failure |
