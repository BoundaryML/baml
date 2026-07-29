// Future compiler lowering:
//   InvestigateRepository.task(ticket, $provider = harness)
// becomes this hand-written InvestigateRepository_task companion.
function InvestigateRepository_task(
    ticket: SupportTicket,
    provider: root.ai.Provider,
) -> root.ai.Task<Resolution> throws never {
    root.ai.task_named<Resolution>(provider, "InvestigateRepository", { "ticket": ticket }, [], (
        ctx: baml.llm.Context,
    ) -> baml.llm.PromptAst {
        baml.llm
            .assemble_prompt_ast(
                ["", "\n", ""],
                [
                    `Investigate this support ticket and propose a resolution.\nTicket: ${ticket.id}\nSubject: ${ticket.subject}\nBody: ${ticket.body}\nCustomer tier: ${ticket.customer_tier}`,
                    ctx.output_format,
                ],
            )
    })
}

// Future compiler lowering for the direct form when the harness implements
// CompletionProvider by owning the complete session lifecycle.
function InvestigateRepository_manual(
    ticket: SupportTicket,
    harness: root.ai.Provider,
) -> Resolution throws root.ai.Failure | baml.errors.UnknownError {
    InvestigateRepository_task(ticket, harness)
        .run(runner = root.ai.run.Completion<Resolution>.new())
}

function fake_model_harness() -> root.ai.harness.ModelHarness throws never {
    root.ai.harness.ModelHarness { inner: fake_resolution(), sessions: [] }
}

function live_openai_model_harness(
) -> root.ai.harness.ModelHarness throws baml.errors.Io | baml.errors.ParseError {
    root.ai.harness.ModelHarness { inner: live_openai(), sessions: [] }
}
