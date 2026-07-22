// Complete seed example for every interface-driven runner migration:
//
//   1. `Task<T>.run` accepts a nominal `Runner<Task<T>>`.
//   2. The concrete runner binds associated Output and Error types.
//   3. The runner delegates to the same provider capability implementation as
//      the function-driven baseline, keeping this an API-shape experiment.
//   4. Both deterministic and live scenarios use the identical call shape.

test "interface runner e2e: typed generation with metadata" {
    let fake = fake_resolution();
    let task = ResolveTicket_task(sample_ticket(), fake);

    let response = task.run(
        runner = root.ai.run.GenerationWithMeta<Resolution>.new(),
    );

    assert.equal(response.value.category, "billing");
    assert.equal(response.meta.provider, "fake");
    assert.equal(fake.calls, 1)
}

test "interface runner e2e: typed value projection" {
    let task = ResolveTicket_task(sample_ticket(), fake_resolution());
    let resolution: Resolution = task.run(
        runner = root.ai.run.Generation<Resolution>.new(),
    );

    assert.equal(resolution.priority, TicketPriority.Urgent)
}

test "interface runner e2e: provider completion" {
    let task = ResolveTicket_task(sample_ticket(), fake_resolution());
    let resolution: Resolution = task.run(
        runner = root.ai.run.Completion<Resolution>.new(),
    );

    assert.equal(resolution.summary, "Duplicate charge")
}

testset "integ-test-interface-runner-e2e" {
    test "interface runner live OpenAI generation" {
        let task = ResolveTicket_task(sample_ticket(), live_openai());
        let response = task.run(
            runner = root.ai.run.GenerationWithMeta<Resolution>.new(),
        );

        log.info(
            "interface runner live generation "
                + {
                    "provider": response.meta.provider,
                    "request_id": response.meta.request_id,
                    "usage": response.meta.usage,
                }
                .to_string(),
        );

        assert.equal(response.meta.provider, "openai");
        assert.is_true((response.meta.usage?.input_tokens ?? 0) > 0);
        assert.is_true(response.value.reply.length() > 0)
    }
}
