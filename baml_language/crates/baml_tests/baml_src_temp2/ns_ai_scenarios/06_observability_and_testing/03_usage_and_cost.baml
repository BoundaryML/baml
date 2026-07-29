// Combine token usage from multiple model calls and estimate their total cost.
function usage_and_cost() -> float throws root.ai.Failure | baml.errors.UnknownError {
    let first = root
        .ai
        .internal
        ._response_from_done(
            root.ai
                .internal
                ._require_done(
                    ResolveTicket_task(sample_ticket(), fake_resolution())
                        .run(runner = root.ai.run.Agent<Resolution>.new()),
                ),
        );
    let second = root
        .ai
        .internal
        ._response_from_done(
            root.ai
                .internal
                ._require_done(
                    ResolveTicket_task(sample_ticket(), fake_resolution())
                        .run(runner = root.ai.run.Agent<Resolution>.new()),
                ),
        );
    let usage = root.ai.observe.aggregate_usage([first.metadata, second.metadata]);
    let estimate = root
        .ai
        .observe
        .estimated_cost(
            usage,
            root.ai.observe.TokenPrice { input_per_million: 1.0, output_per_million: 2.0 },
        );
    log.info({ "usage": usage, "estimated_cost_usd": estimate });
    estimate
}
