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
        [`Investigate ticket ${ticket.id} and propose a resolution.`, ctx.output_format],
      )
    },
  )
}

// Future compiler lowering for the direct form when the harness implements
// DriveProvider by owning the complete session lifecycle.
function InvestigateRepository_manual(
  ticket: SupportTicket,
  harness: root.ai.Provider,
) -> Resolution throws baml.errors.UnknownError | baml.errors.Unsupported {
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
