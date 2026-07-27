# Conversations and provider identity

Portable messages and exact provider continuation are different values.

## Message history

`MessageHistory` is application-owned:

```baml
class MessageHistory {
  messages: ai.Message[],
}
```

It can be edited, summarized, stored, and sent to another provider. It may
contain portable representations of tool calls and results, but it does not
promise byte-for-byte continuation.

## Conversation

`Conversation<P>` is provider-owned exact state:

```baml
// Conceptual shape. Payload is opaque.
class Conversation<P> {
  owner: ai.ProviderIdentity,
  payload: ai.internal.OpaqueConversationState,
}
```

It may preserve:

- provider response IDs;
- tool-call IDs;
- encrypted or hidden reasoning state;
- cache coordinates;
- model and protocol version;
- turn ordering;
- usage totals; and
- provider-specific continuation tokens.

The payload is not application-editable.

## Stable ownership

Provider identity is not a display name. It must distinguish configurations
that cannot safely continue each other's state.

At minimum, identity accounts for:

- provider family and protocol;
- account or endpoint boundary when relevant;
- deployment or model constraints that affect continuation;
- provider state format version; and
- injected transport identity where it changes the authority boundary.

A provider may define that two configured values share continuation identity,
but the default is exact-instance configuration identity.

## Resuming an Agent

The standard Agent accepts an optional conversation:

```baml
ai.run.Agent.new(
  conversation = previous,
)
```

At compile time, `Conversation<P>` should line up with `Task<T, P>` whenever
the concrete types are known. At runtime, the provider verifies the stable
owner identity before any request.

There is no separate `ResumableAgent` runner. Resumption is an optional feature
of the same lifecycle.

## Moving between providers

One provider's `Conversation` is never cast to another provider's
conversation. Switching providers is an explicit import:

```baml
let exported: ai.MessageHistory =
  source.export_messages(conversation)

let imported: ai.ConversationImport<Destination> =
  destination.import_messages(exported)
```

The import reports fidelity:

| Fidelity | Meaning |
| --- | --- |
| `Exact` | Destination guarantees equivalent continuation |
| `MessagesOnly` | Portable visible messages were retained |
| `Lossy` | Some content was summarized, removed, or transformed |

Cross-provider import will normally be `MessagesOnly` or `Lossy`, not `Exact`.

## Saving exact state

If a provider supports durable continuation, it may seal a conversation into
an opaque, versioned token:

```baml
let token: ai.ConversationToken =
  provider.save(conversation)

let restored: ai.Conversation<P> =
  provider.restore(token)
```

The serialized token is not a generic message format. It retains provider
identity and expires or fails clearly if the provider can no longer restore
it.

## Forking

Forking exact state is a provider capability, not a shallow class copy:

```baml
let branch = provider.fork(conversation)
```

If exact forking is unavailable, the application can export messages and
create a new conversation with declared fidelity.
