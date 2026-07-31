// Complete seed example for every interface-driven runner migration:
//
//   1. `Task<T>.run` accepts a nominal `Runner<Task<T>>`.
//   2. The concrete runner binds associated Output and Error types.
//   3. The Agent owns the loop and delegates one model turn at a time to the
//      provider.
//   4. Both deterministic and live scenarios use the identical call shape.

test "interface runner e2e: typed Agent result with metadata" {
    let fake = fake_resolution();
    let task = ResolveTicket@task(sample_ticket()).with_provider(fake);

    let response = (
            done_or_fail(task.run(runner = ai.run.Agent<Resolution>.new()))
        ).response();

    assert.equal(response.value.category, "billing");
    assert.equal(response.metadata.provider, "fake");
    assert.equal(fake.calls, 1)
}

test "interface runner e2e: typed value projection" {
    let task = ResolveTicket@task(sample_ticket()).with_provider(fake_resolution());
    let resolution: Resolution = task.complete(runner = ai.run.Agent<Resolution>.new());

    assert.equal(resolution.priority, TicketPriority.Urgent)
}

test "interface runner e2e: provider-backed Agent result" {
    let task = ResolveTicket@task(sample_ticket()).with_provider(fake_resolution());
    let resolution: Resolution = task.complete(runner = ai.run.Agent<Resolution>.new());

    assert.equal(resolution.summary, "Duplicate charge")
}

testset "integ-test-interface-runner-e2e" {
    test "interface runner live OpenAI Agent" {
        let task = ResolveTicket@task(sample_ticket()).with_provider(live_openai());
        let response = (
                done_or_fail(task.run(runner = ai.run.Agent<Resolution>.new()))
            ).response();

        log.info(
            "interface runner live Agent "
                + {
                    "provider": response.metadata.provider,
                    "request_id": response.metadata.request_id,
                    "usage": response.metadata.usage,
                }
                .to_string(),
        );

        assert.equal(response.metadata.provider, "openai");
        assert.is_true((response.metadata.usage?.input_tokens ?? 0) > 0);
        assert.is_true(response.value.reply.length() > 0)
    }
}
