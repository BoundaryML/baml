// Combine token usage from multiple model calls and estimate their total cost.
function usage_and_cost() -> float throws ai.Failure | baml.errors.UnknownError {
    let first = ai
        .internal
        ._response_from_done(
            ai
                .internal
                ._require_done(
                    ResolveTicket@task(sample_ticket()).with_provider(fake_resolution())
                        .run(runner = ai.run.Agent<Resolution>.new()),
                ),
        );
    let second = ai
        .internal
        ._response_from_done(
            ai
                .internal
                ._require_done(
                    ResolveTicket@task(sample_ticket()).with_provider(fake_resolution())
                        .run(runner = ai.run.Agent<Resolution>.new()),
                ),
        );
    let usage = ai.observe.aggregate_usage([first.metadata, second.metadata]);
    let estimate = ai
        .observe
        .estimated_cost(
            usage,
            ai.observe.TokenPrice { input_per_million: 1.0, output_per_million: 2.0 },
        );
    log.info({ "usage": usage, "estimated_cost_usd": estimate });
    estimate
}
