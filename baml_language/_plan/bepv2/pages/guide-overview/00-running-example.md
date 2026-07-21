# Running example: customer support

> **Status:** Implemented in the executable reference.

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

class Order {
  id: string,
  customer_id: string,
  status: string,
  expected_delivery: string?,
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

The guide uses this sample value whenever a recipe only needs a ticket:

```baml
let ticket = Ticket {
  id: "T-1042",
  customer_id: "C-7",
  message: "Where is order O-42?",
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

## Shared provider setup

The recipes use role names so the support domain does not depend on one
vendor. Define the roles once in your package:

```baml
let FastModel = ai.OpenAi {
  model: "gpt-5-mini",
  api_key: baml.env.get_or_panic("OPENAI_API_KEY"),
  base_url: null,
  extra_headers: null,
  extra_body: null,
}

let CarefulModel = ai.Anthropic {
  model: "claude-sonnet-4-5",
  api_key: baml.env.get_or_panic("ANTHROPIC_API_KEY"),
  base_url: null,
  extra_headers: null,
  extra_body: null,
}

// These providers support tool-aware turns as well as normal typed calls.
let FastToolModel = FastModel
let CarefulToolModel = CarefulModel
let ToolModel = FastToolModel
let SupportModel = FastModel

let RealtimeModel = ai.OpenAiRealtime {
  model: "gpt-realtime",
  voice: "alloy",
  api_key: baml.env.get_or_panic("OPENAI_API_KEY"),
  output_audio: true,
}
```

The exact models are examples, not part of the BEP. A provider matrix can bind
the same task to other concrete implementations without changing the task.

Agent recipes use `ai.AgentOptions.new(...)`. Its named parameters default to an
empty policy, so each snippet names only the settings it changes while the
constructor still returns a complete `AgentOptions` value.

## Application functions used in recipes

Names such as `orders.lookup`, `policies.search`, `refunds.issue`,
`queue_for_review`, and `route_handoff` represent normal application code.
They are not hidden `ai` APIs. Each recipe defines the typed tool wrapper or
driver contract that is new to that page; your application supplies the
database, queue, UI, or business operation behind it.

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
