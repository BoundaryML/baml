function InvestigateRepository(ticket: SupportTicket) -> Resolution {
    provider: fake_resolution()
    prompt: `
        Investigate this support ticket and propose a resolution.
        Ticket: ${ticket.id}
        Subject: ${ticket.subject}
        Body: ${ticket.body}
        Customer tier: ${ticket.customer_tier}
        ${ctx.output_format}
    `
}

function fake_model_harness() -> ai.harness.ModelHarness throws never {
    ai.harness.ModelHarness { inner: fake_resolution(), sessions: [] }
}

function live_openai_model_harness(
) -> ai.harness.ModelHarness throws baml.errors.Io | baml.errors.ParseError {
    ai.harness.ModelHarness { inner: live_openai(), sessions: [] }
}
