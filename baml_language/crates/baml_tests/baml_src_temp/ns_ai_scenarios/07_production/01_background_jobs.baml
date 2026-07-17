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

testset "integ-test-guide-07-background" {
  test "guide 07.01 live OpenAI: submit, persist token, resume, and poll remote work" {
    let provider = root.ai.OpenAiBackground {
      model: "gpt-5.6-luna",
      api_key: baml.env.get_or_panic("OPENAI_API_KEY"),
      base_url: null,
    };
    let submitted = root.ai.drivers.submit_background<Resolution>(
      ResolveTicket_task(sample_ticket(), provider),
      root.ai.BackgroundOptions {
        idempotency_key: "guide-07-01-T-100-" + baml.sys.now_ms().to_string(),
      },
    );
    let persisted = submitted.token();
    assert.equal(persisted.provider, "openai-background")
    assert.is_true(persisted.id.length() > 0)
    // A fresh handle models another process using only the token and its own
    // configured provider. cleanup() is automatic when each handle leaves scope.
    let resumed = provider.resume_job<Resolution>(persisted);
    assert.equal(resumed.token().id, persisted.id)
    let complete: root.ai.Response<Resolution>? = null;
    let attempts = 0;
    let started_ms = baml.sys.now_ms();
    let deadline_ms = started_ms + 60000;
    while (complete == null && baml.sys.now_ms() < deadline_ms) {
      complete = resumed.poll();
      attempts = attempts + 1;
      if (complete == null) {
        baml.sys.sleep(baml.time.Duration.from_milliseconds(500)) catch (e) { _ => null };
      }
    }
    let response = match (complete) {
      let value: root.ai.Response<Resolution> => value,
      null => baml.sys.panic("background response did not complete within 60 seconds"),
    };
    log.info(baml.json.stringify({
      "guide": "07.01",
      "job_id": persisted.id,
      "polls": attempts,
      "elapsed_ms": baml.sys.now_ms() - started_ms,
      "provider": response.meta.provider,
      "input_tokens": response.meta.usage?.input_tokens ?? 0,
    }));
    assert.equal(resumed.status(), root.ai.JobStatus.Complete)
    assert.equal(response.meta.request_id, persisted.id)
    assert.equal(response.meta.provider, "openai-background")
    assert.is_true(response.value.reply.length() > 0)
  }
}
