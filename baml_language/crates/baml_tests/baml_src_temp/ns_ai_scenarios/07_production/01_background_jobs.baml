test "guide 07.01: a background job has poll, cancel, and resume coordinates" {
  let provider = root.ai.FakeBackgroundProvider { inner: fake_resolution() };
  let job = root.ai.drivers.submit_background<Resolution>(
    ResolveTicket_task(sample_ticket(), provider),
    root.ai.BackgroundOptions { idempotency_key: "ticket-T-100" },
  );
  assert.equal(job.status(), root.ai.JobStatus.Complete)
  assert.equal(job.poll()?.value.category, "billing")
  assert.equal(job.token().id, "ticket-T-100")
  job.cleanup();
  job.cleanup();
  assert.equal(job.status(), root.ai.JobStatus.Cancelled)
}

testset "integ-test-guide-07-background-task-path" {
  test "guide 07.01 live OpenAI: the same task executes on a real provider" {
    let value = ResolveTicket_manual(sample_ticket(), root.ai.OpenAi {
      ...root.ai.openai(),
      model: "gpt-5.6-luna",
      api_key: baml.env.get_or_panic("OPENAI_API_KEY"),
    });
    assert.is_true(value.reply.length() > 0)
  }
}
