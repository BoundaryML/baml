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

function fake_resolution() -> root.ai.testing.FakeProvider throws never {
  root.ai.testing.fake_output_provider(`{
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

// Future compiler lowering:
//   ResolveTicket.task(ticket, $provider = provider)
// becomes this hand-written ResolveTicket_task companion. It creates a task
// and performs no model I/O.
function ResolveTicket_task(
  ticket: SupportTicket,
  provider: root.ai.Provider,
) -> root.ai.Task<Resolution> throws never {
  root.ai.task_named<Resolution>(
    provider,
    "ResolveTicket",
    { "ticket": ticket },
    [],
    (ctx: baml.llm.Context) -> baml.llm.PromptAst {
      baml.llm.assemble_prompt_ast(
        ["", "\n", ""],
        [
          `Resolve this support ticket.\nSubject: ${ticket.subject}\nBody: ${ticket.body}`,
          ctx.output_format,
        ],
      )
    },
  )
}

// Future compiler lowering:
//   ResolveTicket(ticket, $provider = provider)
// becomes an ordinary bounded Agent run over ResolveTicket_task(ticket, provider).
function ResolveTicket_manual(
  ticket: SupportTicket,
  provider: root.ai.Provider,
) -> Resolution throws root.ai.Failure | baml.errors.UnknownError {
  root.ai.internal._require_done(
    ResolveTicket_task(ticket, provider)
      .run(runner = root.ai.run.Agent<Resolution>.new()),
  ).value
}

function drain_resolution_stream(
  task: root.ai.Task<Resolution>,
) -> Resolution throws baml.errors.UnknownError | root.ai.Failure
    | baml.errors.Unsupported | baml.errors.Io | baml.errors.LlmClient {
  let stream = task.run(
    runner = root.ai.run.Stream<Resolution$stream, Resolution>.new(),
  );
  // final() drains the provider's SSE response and validates the typed result.
  stream.final()
}
