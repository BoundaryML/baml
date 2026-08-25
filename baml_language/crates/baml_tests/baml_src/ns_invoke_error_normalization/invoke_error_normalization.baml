function normalized_kind(error: ai.errors.RaisedInClient) -> string {
    match (ai.errors.normalize_invoke(error)) {
        let timeout: baml.errors.Timeout => {
            `Timeout:${timeout.message}:${timeout.duration_ms ?? -1}`
        },
        let cancelled: baml.panics.Cancelled => `Cancelled:${cancelled.message}`,
        let network: ai.errors.NetworkFailure => `NetworkFailure:${network.detail}`,
        let unknown_error: baml.errors.UnknownError => {
            `UnknownError:${unknown_error.message.join("|")}`
        },
        _ => "other",
    }
}

function TimeoutSpec() -> string {
    client: "openai/gpt-4o-mini"
    prompt: `Return one string. ${ctx.output_format}`
}

class TimeoutInvocationProbe {
    count: int,
}

class TimeoutInvokeClient {
    timeout: baml.errors.Timeout,
    probe: TimeoutInvocationProbe,

    implements ai.Client {
        function id(self) -> string {
            "timeout-provider"
        }

        function render(self, input: ai.ModelTurnInput) -> baml.http.Request {
            let _ = input;
            baml.http.Request {
                method: "POST",
                url: "https://example.invalid/timeout",
                headers: {},
                body: "{}",
            }
        }

        function invoke(self, input: ai.ModelTurnInput) -> ai.ModelTurn {
            let _ = input;
            self.probe.count += 1;
            throw ai.errors.normalize_invoke(self.timeout)
        }
    }
}

function timeout_input() -> ai.ModelTurnInput {
    let spec = TimeoutSpec@spec();
    ai.ModelTurnInput {
        prompt: spec.prompt_template,
        journal: ai.Journal.new(spec),
        toolbox: spec.tools(),
        output_type: spec.output_type(),
    }
}

function capture_client_timeout(client: ai.Client) -> unknown {
    client.invoke(timeout_input()) catch_all (e) {
        _ => e,
    }
}

function capture_agent_timeout(client: ai.Client) -> unknown {
    ai.Agent.new(client = client).run(TimeoutSpec@spec()) catch_all (e) {
        _ => e,
    }
}

function timeout_detail(error: unknown) -> string {
    match (error) {
        let timeout: baml.errors.Timeout => `${timeout.message}:${timeout.duration_ms ?? -1}`,
        _ => `unexpected:${error.to_string()}`,
    }
}

test "normalize_invoke_preserves_cancelled_identity" {
    let original = baml.panics.Cancelled { message: "before" };
    let normalized = ai.errors.normalize_invoke(original);
    original.message = "after";
    assert.equal(normalized_kind(normalized), "Cancelled:after")
}

test "normalize_invoke_preserves_timeout_identity" {
    let original = baml.errors.Timeout { message: "before", duration_ms: 10 };
    let normalized = ai.errors.normalize_invoke(original);
    original.message = "after";
    original.duration_ms = 20;
    assert.equal(normalized_kind(normalized), "Timeout:after:20")
}

test "normalize_invoke_preserves_failure_identity" {
    let original = ai.errors.NetworkFailure { status_code: null, provider: "test", detail: "before", raw_body: null };
    let normalized = ai.errors.normalize_invoke(original);
    original.detail = "after";
    assert.equal(normalized_kind(normalized), "NetworkFailure:after")
}

test "normalize_invoke_preserves_unknown_error_identity" {
    let original = baml.errors.UnknownError { data: "payload", message: ["before"] };
    let normalized = ai.errors.normalize_invoke(original);
    original.message = ["after"];
    assert.equal(normalized_kind(normalized), "UnknownError:after")
}

// Raw infrastructure types are CLASSIFIED into typed failures, never wrapped
// in UnknownError: a config bug arrives as InvalidRequest with the message
// preserved in `detail`.
test "normalize_invoke_classifies_config_errors" {
    let original = baml.errors.InvalidArgument { message: "invalid" };
    let normalized = ai.errors.normalize_invoke(original);
    match (normalized) {
        let invalid: ai.errors.InvalidRequest => {
            assert.equal(invalid.provider, "client");
            assert.equal(invalid.detail, "invalid")
        },
        _ => assert.is_true(false),
    }
}

test "Client and Agent preserve typed timeout" {
    let timeout = baml.errors.Timeout { message: "deadline elapsed", duration_ms: 30000 };
    let direct_probe = TimeoutInvocationProbe { count: 0 };
    let direct = TimeoutInvokeClient { timeout: timeout, probe: direct_probe };
    let direct_error = capture_client_timeout(direct);
    let agent_probe = TimeoutInvocationProbe { count: 0 };
    let agent_client = TimeoutInvokeClient { timeout: timeout, probe: agent_probe };
    let agent_error = capture_agent_timeout(agent_client);

    timeout.message = "deadline still elapsed";
    assert.equal(timeout_detail(direct_error), "deadline still elapsed:30000");
    assert.equal(timeout_detail(agent_error), "deadline still elapsed:30000");
    assert.equal(direct_probe.count, 1);
    assert.equal(agent_probe.count, 1)
}

test "non-stream composed clients preserve typed timeout without replay" {
    let retry_probe = TimeoutInvocationProbe { count: 0 };
    let retry_inner = TimeoutInvokeClient {
        timeout: baml.errors.Timeout { message: "retry-timeout", duration_ms: 100 },
        probe: retry_probe,
    };
    let retry = ai.clients.Retry.new(
        retry_inner,
        max_attempts = 3,
        backoff = ai.clients.Backoff.new(initial_ms = 0, max_ms = 0),
    );
    let retry_error = capture_client_timeout(retry);

    let round_robin_probe = TimeoutInvocationProbe { count: 0 };
    let round_robin_inner = TimeoutInvokeClient {
        timeout: baml.errors.Timeout { message: "round-robin-timeout", duration_ms: 200 },
        probe: round_robin_probe,
    };
    let round_robin = ai.clients.RoundRobin.new([round_robin_inner]);
    let round_robin_error = capture_client_timeout(round_robin);

    let fallback_probe = TimeoutInvocationProbe { count: 0 };
    let fallback_inner = TimeoutInvokeClient {
        timeout: baml.errors.Timeout { message: "fallback-timeout", duration_ms: 300 },
        probe: fallback_probe,
    };
    let backup_probe = TimeoutInvocationProbe { count: 0 };
    let backup = TimeoutInvokeClient {
        timeout: baml.errors.Timeout { message: "backup-timeout", duration_ms: 400 },
        probe: backup_probe,
    };
    let fallback = ai.clients.Fallback { members: [fallback_inner, backup] };
    let fallback_error = capture_client_timeout(fallback);

    assert.equal(timeout_detail(retry_error), "retry-timeout:100");
    assert.equal(timeout_detail(round_robin_error), "round-robin-timeout:200");
    assert.equal(timeout_detail(fallback_error), "fallback-timeout:300");
    assert.equal(retry_probe.count, 1);
    assert.equal(round_robin_probe.count, 1);
    assert.equal(fallback_probe.count, 1);
    assert.equal(backup_probe.count, 0)
}
