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

function fake_resolution() -> root.ai.FakeProvider throws never {
  root.ai.fake_output_provider(`{
    "category": "billing",
    "priority": "Urgent",
    "summary": "Duplicate charge",
    "reply": "We will investigate order O-42."
  }`)
}

function live_openai() -> root.ai.OpenAi throws baml.errors.Io | baml.errors.ParseError {
  root.ai.OpenAi {
    ...root.ai.openai(),
    model: "gpt-5.6-luna",
    api_key: baml.env.get_or_panic("OPENAI_API_KEY"),
  }
}

function live_anthropic() -> root.ai.Anthropic throws baml.errors.Io | baml.errors.ParseError {
  root.ai.Anthropic {
    ...root.ai.anthropic(),
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
// becomes an ordinary completion run over ResolveTicket_task(ticket, provider).
function ResolveTicket_manual(
  ticket: SupportTicket,
  provider: root.ai.Provider,
) -> Resolution throws root.ai.CallError | baml.errors.UnknownError {
  root.ai.drivers.complete<Resolution>(ResolveTicket_task(ticket, provider))
}

function drain_resolution_stream(
  task: root.ai.Task<Resolution>,
) -> Resolution throws baml.errors.UnknownError | root.ai.StreamError
    | baml.errors.Unsupported | baml.errors.Io | baml.errors.LlmClient {
  // Concrete execution preserves the exact throws union inside an explicitly
  // annotated helper; Task.run currently leaks its generic Error here.
  let runner = root.ai.run.Stream<Resolution$stream, Resolution>.new();
  let stream = runner.run(task);
  // final() drains the provider's SSE response and validates the typed result.
  stream.final()
}
