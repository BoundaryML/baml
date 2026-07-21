// Complete seed example for every interface-driven driver migration:
//
//   1. `Task<T>.drive` accepts a nominal `Driver<Task<T>>`.
//   2. The concrete driver binds associated Output and Error types.
//   3. The driver delegates to the same provider capability implementation as
//      the function-driven baseline, keeping this an API-shape experiment.
//   4. Both deterministic and live scenarios use the identical call shape.

test "interface driver e2e: typed generation with metadata" {
  let fake = fake_resolution();
  let task = ResolveTicket_task(sample_ticket(), fake);

  let response = task.drive(
    root.ai.driver.generate_with_meta<Resolution>(),
  );

  assert.equal(response.value.category, "billing")
  assert.equal(response.meta.provider, "fake")
  assert.equal(fake.calls, 1)
}

test "interface driver e2e: typed value projection" {
  let task = ResolveTicket_task(sample_ticket(), fake_resolution());
  let resolution: Resolution = task.drive(
    root.ai.driver.generate<Resolution>(),
  );

  assert.equal(resolution.priority, TicketPriority.Urgent)
}

test "interface driver e2e: provider default drive" {
  let task = ResolveTicket_task(sample_ticket(), fake_resolution());
  let resolution: Resolution = task.drive(
    root.ai.driver.drive<Resolution>(),
  );

  assert.equal(resolution.summary, "Duplicate charge")
}

testset "integ-test-interface-driver-e2e" {
  test "interface driver live OpenAI generation" {
    let task = ResolveTicket_task(sample_ticket(), live_openai());
    let response = task.drive(
      root.ai.driver.generate_with_meta<Resolution>(),
    );

    log.info("interface driver live generation " + {
      "provider": response.meta.provider,
      "request_id": response.meta.request_id,
      "usage": response.meta.usage,
    }.to_string());

    assert.equal(response.meta.provider, "openai")
    assert.is_true((response.meta.usage?.input_tokens ?? 0) > 0)
    assert.is_true(response.value.reply.length() > 0)
  }
}
