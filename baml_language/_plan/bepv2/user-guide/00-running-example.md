# Running example: customer support

Most of this guide grows one customer-support application. The domain is
simple enough to recognize immediately but rich enough to demonstrate typed
outputs, tools, effects, handoffs, provider switching, sessions, and realtime
voice.

## Stable task model

```baml
enum Intent {
  OrderStatus
  Refund
  ProductQuestion
  Other
}

class Ticket {
  id: string,
  customer_id: string,
  message: string,
}

class Resolution {
  intent: Intent,
  reply: string,
  resolved: bool,
}

function ResolveTicket(ticket: Ticket) -> Resolution {
  provider: SupportModel
  prompt: `
    Resolve this customer-support ticket.

    Ticket: ${ticket}
    ${ctx.output_format}
  `
}
```

The first example calls `ResolveTicket(ticket)` directly. Later examples turn
the same invocation into a task value with `ResolveTicket.task(ticket)` and
choose streaming, agent, background, session, or custom drivers explicitly.

## Tools introduced by later chapters

```text
lookup_order       read-only lookup
search_policy      read-only retrieval
issue_refund       effectful business operation
transfer_to_human  terminal handoff
tool_search        discovers or authorizes more tools
```

These names stay stable so a chapter can focus on the execution change:
adding a second tool, running calls concurrently, blocking an effectful call,
or updating the registry halfway through the conversation.

## Provider progression

The guide uses descriptive provider values rather than making the domain
depend on one vendor:

```text
FastModel       inexpensive default
CarefulModel    stronger fallback
ToolModel       supports provider turns containing tool calls
RealtimeModel   opens a live duplex resource
```

Provider-specific chapters may instantiate OpenAI, Anthropic, Gemini, or a
custom adapter behind these roles. Provider matrix tests bind the same task to
each concrete implementation without changing the task declaration.

## Ownership remains constant

```text
application owns: Ticket data, tool execution, UI, logs, and business state
task owns:        prompt intent and the Resolution output contract
driver owns:      execution lifecycle, looping, and terminal outcomes
provider owns:    wire protocol and exact continuation transcript
```

Later chapters add resources without changing this boundary. Application-owned
history remains editable `Conversation` data. Provider-owned sessions return
opaque resume tokens. Realtime interaction returns a `Live` resource while the
caller retains its input/output `Channel`.

## Deliberate exceptions

External coding-harness chapters use a small code-editing task because
pretending Claude Code is a customer-support backend would make the example
less clear. Those chapters preserve the same task/driver/provider ownership
model even though the domain changes.
