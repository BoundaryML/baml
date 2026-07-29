test "guide 07.01: a background job has poll, cancel, and resume coordinates" {
    let provider = root.ai.testing.FakeBackgroundProvider { inner: fake_resolution() };
    let job = ResolveTicket_task(sample_ticket(), provider)
        .run(runner = root.ai.run.Background<Resolution>.new(idempotency_key = "ticket-T-100"));
    assert.equal(job.status(), root.ai.jobs.JobStatus.Complete);
    assert.equal(job.poll()?.value.category, "billing");
    assert.equal(job.token().id, "ticket-T-100");
    // job.cleanup();
    // job.cleanup();
    // assert.equal(job.status(), root.ai.jobs.JobStatus.Cancelled)
}

testset "integ-test-guide-07-background" {
    test "guide 07.01 live OpenAI: submit, persist token, resume, and poll remote work" {
        // The full integration selector intentionally launches many foreground
        // provider calls in parallel. Submit background-priority work after that
        // burst so this test measures the job lifecycle rather than account-level
        // queue starvation.
        baml.sys.sleep(baml.time.Duration.from_seconds(15)) catch (e) {
            _ => null
        };
        let provider = root.openai.OpenAIProvider {
            ...root.openai.responses(),
            model: "gpt-5.6-luna",
            api_key: baml.env.get_or_panic("OPENAI_API_KEY"),
            base_url: null,
        };
        let submitted = ResolveTicket_task(sample_ticket(), provider)
            .run(
                runner = root
                    .ai
                    .run
                    .Background<Resolution>
                    .new(idempotency_key = "guide-07-01-T-100-" + baml.sys.now_ms().to_string()),
            );
        let persisted = submitted.token();
        assert.equal(persisted.provider, "openai");
        assert.is_true(persisted.id.length() > 0);
        // A fresh handle models another process using only the token and its own
        // configured provider.
        let resumed = provider.resume_job<Resolution>(persisted);
        assert.equal(resumed.token().id, persisted.id);
        let complete: root.ai.ResponseWithMetadata<Resolution>? = null;
        let attempts = 0;
        let poll_errors = 0;
        let started_ms = baml.sys.now_ms();
        let deadline_ms = started_ms + 60000;
        while (complete == null && baml.sys.now_ms() < deadline_ms) {
            complete
                = resumed.poll() catch (e) {
                    // Polling is read-only and replay-safe. A transient transport failure
                    // must not discard the durable response ID or force resubmission.
                    _ => {
                        if (resumed.status() == root.ai.jobs.JobStatus.Failed) {
                            throw e;
                        }
                        poll_errors = poll_errors + 1;
                        null
                    },
                };
            attempts = attempts + 1;
            if (complete == null) {
                baml.sys.sleep(baml.time.Duration.from_milliseconds(500)) catch (e) {
                    _ => null
                };
            }
        }
        let response = match (complete) {
            let value: root.ai.ResponseWithMetadata<Resolution> => value,
            null => baml.sys.panic("background response did not complete within 60 seconds"),
        };
        log.info(
            baml.json
                .stringify(
                    {
                        "guide": "07.01",
                        "job_id": persisted.id,
                        "polls": attempts,
                        "poll_errors": poll_errors,
                        "elapsed_ms": baml.sys.now_ms() - started_ms,
                        "provider": response.metadata.provider,
                        "input_tokens": response.metadata.usage?.input_tokens ?? 0,
                    },
                ),
        );
        assert.equal(resumed.status(), root.ai.jobs.JobStatus.Complete);
        assert.equal(response.metadata.request_id, persisted.id);
        assert.equal(response.metadata.provider, "openai");
        assert.is_true(response.value.reply.length() > 0)
    }
}
