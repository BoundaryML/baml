# Fakes and failure injection

Use deterministic providers to test orchestration. Live models test adapter
integration, not every branch of application control flow.

## Script a provider

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

`ai.testing.*` names above describe the intended testing surface; exact
constructors remain standard-library design work.

## Related design and scenarios

- Fake providers appear throughout scenarios 01–42; scenarios 29 and 39 cover
  failure and hook behavior particularly well.
