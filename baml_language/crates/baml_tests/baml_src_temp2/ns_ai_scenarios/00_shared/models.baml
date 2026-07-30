enum TicketPriority {
  Low
  Normal
  Urgent
}

class SupportTicket {
  id: string,
  subject: string,
  body: string,
  customer_tier: string,
}

class Resolution {
  category: string,
  priority: TicketPriority,
  summary: string,
  reply: string,
}

class KnowledgeLookupArgs {
  query: string,
}

class AccountLookupArgs {
  customer_id: string,
}

function sample_ticket() -> SupportTicket throws never {
  SupportTicket {
    id: "T-100",
    subject: "Charged twice",
    body: "I see two charges for order O-42.",
    customer_tier: "pro",
  }
}

function fake_resolution() -> ai.testing.FakeProvider throws never {
  ai.testing.fake_output_provider(`{
    "category": "billing",
    "priority": "Urgent",
    "summary": "Duplicate charge",
    "reply": "We will investigate order O-42."
  }`)
}

// Named model values used throughout the BEP pages: the fast default and the
// careful escalation target. Thin aliases over the live constructors so doc
// snippets and scenarios share one vocabulary.
function fast_model() -> root.openai.OpenAIProvider throws baml.errors.Io | baml.errors.ParseError {
  live_openai()
}

function careful_model() -> root.anthropic.AnthropicProvider throws baml.errors.Io | baml.errors.ParseError {
  live_anthropic()
}

function live_openai() -> root.openai.OpenAIProvider throws baml.errors.Io | baml.errors.ParseError {
  root.openai.OpenAIProvider {
    ...root.openai.responses(),
    model: "gpt-5.6-luna",
    api_key: baml.env.get_or_panic("OPENAI_API_KEY"),
  }
}

function live_anthropic() -> root.anthropic.AnthropicProvider throws baml.errors.Io | baml.errors.ParseError {
  root.anthropic.AnthropicProvider {
    ...root.anthropic.messages(),
    model: "claude-sonnet-4-6",
    api_key: baml.env.get_or_panic("ANTHROPIC_API_KEY"),
  }
}

function ResolveTicket(ticket: SupportTicket) -> Resolution {
  provider: fake_resolution()
  prompt: `
    Resolve this support ticket.
    Subject: ${ticket.subject}
    Body: ${ticket.body}
    ${ctx.output_format}
  `
}

function drain_resolution_stream(
  task: ai.Task<Resolution>,
) -> Resolution throws baml.errors.UnknownError | ai.Failure
    | baml.errors.Unsupported | baml.errors.Io | baml.errors.LlmClient {
  let stream = task.run(
    runner = ai.run.Stream<Resolution$stream, Resolution>.new(),
  );
  // final() drains the provider's SSE response and validates the typed result.
  stream.final()
}
