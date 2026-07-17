// Future compiler lowering:
//   InvestigateRepository.task(ticket, $provider = harness)
// becomes this hand-written InvestigateRepository_task companion.
function InvestigateRepository_task(
  ticket: SupportTicket,
  provider: root.ai.Provider,
) -> root.ai.Task<Resolution> throws never {
  root.ai.task_named<Resolution>(
    provider,
    "InvestigateRepository",
    { "ticket": ticket },
    [],
    default_task_options(),
    (ctx: baml.llm.Context) -> baml.llm.PromptAst {
      baml.llm.assemble_prompt_ast(
        ["", "\n", ""],
        [
          `Investigate this support ticket and propose a resolution.\nTicket: ${ticket.id}\nSubject: ${ticket.subject}\nBody: ${ticket.body}\nCustomer tier: ${ticket.customer_tier}`,
          ctx.output_format,
        ],
      )
    },
  )
}

// Future compiler lowering for the direct form when the harness implements
// DriveProvider by owning the complete session lifecycle.
function InvestigateRepository_manual(
  ticket: SupportTicket,
  harness: root.ai.Provider,
) -> Resolution throws baml.errors.CallError | baml.errors.UnknownError {
  root.ai.drivers.drive<Resolution>(InvestigateRepository_task(ticket, harness))
}

function harness_options() -> root.ai.HarnessOptions throws never {
  root.ai.HarnessOptions {
    cwd: "/workspace",
    permission_mode: "read-only",
    sandbox: "workspace",
    attributes: { "team": "support" },
  }
}

function fake_model_harness() -> root.ai.ModelHarness throws never {
  root.ai.ModelHarness { inner: fake_resolution(), sessions: [] }
}

function live_openai_model_harness() -> root.ai.ModelHarness
    throws baml.errors.Io | baml.errors.ParseError {
  root.ai.ModelHarness { inner: live_openai(), sessions: [] }
}
