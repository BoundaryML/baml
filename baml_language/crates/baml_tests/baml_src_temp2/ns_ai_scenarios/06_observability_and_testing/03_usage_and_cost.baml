// Combine token usage from multiple model calls and estimate their total cost.
function usage_and_cost() -> float throws root.ai.CallError | baml.errors.UnknownError {
    let first = ResolveTicket_task(sample_ticket(), fake_resolution())
        .run(runner = root.ai.run.GenerationWithMeta<Resolution>.new());
    let second = ResolveTicket_task(sample_ticket(), fake_resolution())
        .run(runner = root.ai.run.GenerationWithMeta<Resolution>.new());
    let usage = root.ai.aggregate_usage([first.meta, second.meta]);
    let estimate = root
        .ai
        .estimated_cost(
            usage,
            root.ai.TokenPrice { input_per_million: 1.0, output_per_million: 2.0 },
        );
    log.info({ "usage": usage, "estimated_cost_usd": estimate });
    estimate
}
