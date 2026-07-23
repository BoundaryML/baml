# Route before the call

> **Status:** Implemented in the executable reference.

Business routing is ordinary application code returning a provider. It is not
a capability and does not belong inside the base `Provider` contract.

## Route by tenant or ticket

```baml
function support_model(account: Account, ticket: Ticket) -> ai.DriveProvider {
  if (account.region == "eu") {
    EuModel
  } else if (ticket.message.length() > 10000) {
    LongContextModel
  } else {
    FastModel
  }
}

let resolution = ResolveTicket(
  ticket,
  $provider = support_model(account, ticket),
)
```

Returning `DriveProvider` preserves the proof required by the direct call.

## Compose routing with generic reliability

```baml
let selected: ai.Provider = support_model_for_runtime_config(account)
let reliable = ai.retry(selected, retry_policy)

let task = ResolveTicket.task(ticket, $provider = reliable)
let resolution = ai.drivers.unsafe.drive(task)
```

Use the free function after intentional type erasure; fluent blanket sugar is
available on concrete providers.

## Cascades are value policy

A cheap model, judge, and stronger model may form a `JudgeGated` provider
wrapper. Its implementation should report all attempts and must not describe a
low judge score as a transport failure.

## Related design


- [Routing is not a combinator](../specification/09-reliability-and-errors.md#routing-is-not-a-combinator)
