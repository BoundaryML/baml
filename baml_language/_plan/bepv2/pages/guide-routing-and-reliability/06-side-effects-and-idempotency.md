# Side effects and idempotency

> **Status:** Implemented in the executable reference.

A transport failure does not prove an operation failed before committing.
Effectful tools and background submissions need explicit replay policy.

## Give effects stable keys

```baml
class RefundArgs {
  ticket_id: string,
  amount_cents: int,
  idempotency_key: string,
}

let issue_refund = ai.tool(
  "issue_refund",
  "Issue an approved refund exactly once.",
  (args: RefundArgs) -> RefundReceipt {
    payments.refund(args)
  },
)
```

The application derives the key from stable business identity, not from an
attempt number:

```baml
let key = `refund:${ticket.id}:${approved_amount}`
```

## Commit states

```text
NotCommitted -> known that no effect happened
Committed    -> known that it happened
Unknown      -> dangerous default; do not duplicate an unkeyed effect
```

Tool middleware should require approval and inject or verify the key before
dispatch. Retry policy must not restart an entire agent run merely because a
later model turn failed after the refund completed.

## Test it

Inject a timeout after the fake payment store records the refund. Replaying
with the same key should return the original receipt; replaying without a key
must be refused.

## Related design


- [Replay policy](../specification/09-reliability-and-errors.md#replay-policy-the-operation-level-half)
