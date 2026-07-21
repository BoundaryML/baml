# Test one task across providers

> **Status:** Implemented in the executable reference.

A provider contract matrix keeps the task, inputs, and assertions fixed while
changing only `$provider`.

## Shared contract helper

```baml
class ProviderCase {
  name: string,
  provider: ai.DriveProvider,
}

function assert_support_contract(case: ProviderCase, ticket: Ticket) -> null {
  let result = ResolveTicket(ticket, $provider = case.provider)

  assert.is_true(result.reply.length() > 0)
  assert.is_true(result.intent == Intent.OrderStatus)
}
```

## Offline matrix

```baml
test "support task obeys its contract across providers" {
  let cases = [
    ProviderCase { name: "openai-fake", provider: FakeOpenAi },
    ProviderCase { name: "anthropic-fake", provider: FakeAnthropic },
    ProviderCase { name: "gemini-fake", provider: FakeGemini },
  ]

  for (let case in cases) {
    assert_support_contract(case, order_status_ticket)
  }
}
```

## Live matrix

Keep credentialed calls in a separately labelled testset. Record provider,
model, latency, usage, and cost for every case. A provider-specific test is
appropriate only when the capability differs—for example, realtime or dynamic
tools—not to duplicate the same typed-output assertion.

## Contract versus quality

This matrix asks whether every provider returns valid `Resolution`. It does not
prove that every answer is equally useful. Quality belongs in an evaluation.
