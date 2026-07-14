# Direct typed call

Start with the LLM function as the application API. The declared return type is
the contract; application code does not parse provider text.

## Use it

```baml
let ticket = Ticket {
  id: "ticket_4821",
  customer_id: "cus_123",
  message: "Where is order 4821?",
}

let resolution: Resolution = ResolveTicket(ticket)
```

`ResolveTicket` either returns a validated `Resolution` or throws. Extraction
and classification are one model interaction here; no application tool has
been called.

## Declaration

```baml
function ResolveTicket(ticket: Ticket) -> Resolution {
  provider: SupportModel
  prompt: `
    Classify and resolve this support ticket.
    Ticket: ${ticket}
    ${ctx.output_format}
  `
}
```

Changing the fields of `Resolution` changes the output contract seen by the
provider adapter. The function body remains provider-independent.

## What the direct call means

Conceptually, the compiler lowers the call through the provider's default
drive behavior:

```baml
ai.drivers.drive(ResolveTicket.task(ticket))
```

The selected provider must implement `DriveProvider`. A basic provider may
drive with one model request; an `Agent` provider may drive a complete tool
loop. Both must finish as `Resolution` or throw.

## Test it

Use a deterministic fake provider for an ordinary unit test:

```baml
test "classifies an order-status ticket" {
  let result = ResolveTicket(
    Ticket {
      id: "ticket_4821",
      customer_id: "cus_123",
      message: "Where is order 4821?",
    },
    $provider = FakeSupportModel,
  )

  assert.equal(result.intent, Intent.OrderStatus)
  assert.is_true(result.reply.length() > 0)
}
```

Keep credentialed model comparisons in live testsets. A passing unit test
should not depend on model sampling.

## Related design and scenarios

- Normative direct-call lowering: [desugaring](../../pages/02-desugaring.md)
- Scenario families: 01 single turn, 02 structured output, 03 constrained decoding
