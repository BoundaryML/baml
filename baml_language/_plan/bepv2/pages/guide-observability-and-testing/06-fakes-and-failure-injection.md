# Fakes and failure injection

> **Status:** Partial — deterministic fake providers work today. The final
> public `ai.testing.*` constructor names are still proposed.

Use deterministic providers to test orchestration. Live models test adapter
integration, not every branch of application control flow.

## Script a provider

The following `ai.testing` names are the proposed public shorthand. The
testing contract is stable even though the constructor names are not final:

```baml
let fake = ai.testing.FakeToolProvider {
  turns: [
    ai.testing.call("lookup_order", { "order_id": "4821" }),
    ai.testing.finish<Resolution>(Resolution {
      intent: Intent.OrderStatus,
      reply: "Order 4821 arrives Tuesday.",
      resolved: true,
    }),
  ],
}
```

## Inject classified failures

```baml
let flaky = ai.testing.FakeDriveProvider {
  attempts: [
    ai.testing.retryable_failure("rate limited"),
    ai.testing.success(expected_resolution),
  ],
}
```

Test retry limits, fallback selection, tool denial, budget stops, transcript
conversion warnings, session ownership, and cleanup without sleeping or using
credentials.

## Assertions that matter

- exact number and order of attempts;
- which provider received each rebound task;
- tool-call/result ID correlation;
- whether a side effect executed once;
- emitted event order; and
- resource cleanup on success and failure.

## Test automatic cleanup deterministically

Explicit `defer { resource.cleanup() }` is already deterministic. To test the
separate GC-finalization path, let the resource become unreachable and then
force a collection:

```baml
create_and_abandon_resource(audit_log)
baml.sys.collect_garbage()
assert.equal(audit_log, ["cleaned"])
```

The call performs a full collection and drains queued `cleanup()` finalizers
before returning. It does not clean reachable resources and should normally be
reserved for tests and runtime diagnostics.

The reference implementation already provides deterministic output, tool-turn,
and failure providers. Standard-library design work remains only for the
public builder names shown above.
