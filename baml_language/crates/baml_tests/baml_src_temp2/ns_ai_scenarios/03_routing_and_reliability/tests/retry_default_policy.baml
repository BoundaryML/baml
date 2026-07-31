// D5 semantics: the default retry judgment matches the documented guidance.
// Transport failures replay (the step contract makes them safe); refusals,
// invalid requests, and parse failures do not (an identical replay gets an
// identical answer). retry_if overrides in either direction.

test "D5: default retry replays a transport failure" {
    let flaky = ai.testing.FakeProvider {
        ...fake_resolution(),
        failures_remaining: 1,
        failure_mode: ai.testing.FakeFailureMode.Network,
    };
    let value = ResolveTicket@task(sample_ticket())
        .with_provider(ai.retry(flaky, 3, backoff = ai.Backoff { initial_ms: 1, multiplier: 2, max_ms: 5 }))
        .complete(runner = ai.run.Agent<Resolution>.new());
    assert.equal(value.category, "billing")
    assert.equal(flaky.calls, 2)
}

test "D5: default retry declines to replay a parse failure" {
    let flaky = ai.testing.FakeProvider {
        ...fake_resolution(),
        failures_remaining: 3,
        failure_mode: ai.testing.FakeFailureMode.Parse,
    };
    let kind = ResolveTicket@task(sample_ticket())
        .with_provider(ai.retry(flaky, 3))
        .complete(runner = ai.run.Agent<Resolution>.new())
        .category catch (e) {
        let parse: ai.ParseFailed => "parse-failed",
        _ => "other",
    };
    assert.equal(kind, "parse-failed")
    //# One call, no blind replays of a terminal failure
    assert.equal(flaky.calls, 1)
}

test "D5: default retry declines to replay a refusal" {
    let flaky = ai.testing.FakeProvider {
        ...fake_resolution(),
        failures_remaining: 3,
        failure_mode: ai.testing.FakeFailureMode.Terminal,
    };
    let kind = ResolveTicket@task(sample_ticket())
        .with_provider(ai.retry(flaky, 3))
        .complete(runner = ai.run.Agent<Resolution>.new())
        .category catch (e) {
        let refused: ai.Refused => "refused",
        _ => "other",
    };
    assert.equal(kind, "refused")
    assert.equal(flaky.calls, 1)
}

test "D5: retry_if overrides the default in both directions" {
    //# Force a replay of a parse failure the default would decline
    let flaky = ai.testing.FakeProvider {
        ...fake_resolution(),
        failures_remaining: 1,
        failure_mode: ai.testing.FakeFailureMode.Parse,
    };
    let value = ResolveTicket@task(sample_ticket())
        .with_provider(ai.retry(flaky, 3, retry_if = (failure: ai.Failure) -> bool { true }, backoff = null))
        .complete(runner = ai.run.Agent<Resolution>.new());
    assert.equal(value.category, "billing")
    assert.equal(flaky.calls, 2)

    //# Decline a transport failure the default would replay
    let stubborn = ai.testing.FakeProvider {
        ...fake_resolution(),
        failures_remaining: 1,
        failure_mode: ai.testing.FakeFailureMode.Network,
    };
    let kind = ResolveTicket@task(sample_ticket())
        .with_provider(ai.retry(stubborn, 3, retry_if = (failure: ai.Failure) -> bool { false }))
        .complete(runner = ai.run.Agent<Resolution>.new())
        .category catch (e) {
        let network: ai.NetworkFailure => "network",
        _ => "other",
    };
    assert.equal(kind, "network")
    assert.equal(stubborn.calls, 1)
}
