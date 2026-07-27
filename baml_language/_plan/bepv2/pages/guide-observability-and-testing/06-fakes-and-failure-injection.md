# Test-only provider fixtures

The reference implementation uses local provider fixtures to exercise Agent
loops, retries, failures, and tool dispatch without credentials. Those values
belong to the conformance suite. Their names and constructors are not part of
the proposed user API.

Applications that need the same depth of deterministic orchestration testing
may implement the relevant provider capability in their own test sources. That
is an ordinary interface implementation owned by the application.

## Assertions that matter

Private conformance fixtures should make it possible to assert:

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
function test_abandoned_resource(audit_log: string[]) -> null {
  create_and_abandon_resource(audit_log);
  baml.sys.collect_garbage();
  assert.equal(audit_log, ["cleaned"])
}
```

The call performs a full collection and drains queued `cleanup()` finalizers
before returning. It does not clean reachable resources and should normally be
reserved for tests and runtime diagnostics.
