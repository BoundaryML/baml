// Offline contract tests for streaming reliability clients. The scripted
// clients fail at the same two boundaries as real providers: while opening a
// stream and while pulling its next event.

function CompositeStreamSpec() -> string {
    client: "openai/gpt-4o-mini"
    prompt: `Return a short value. ${ctx.output_format()}`
}

class StreamProbe {
    events: string[],
}

class ScriptedStreamClient {
    client_id: string,
    chunks: string[],
    fail_open_attempts: int,
    fail_pull_attempts: int,
    fail_after_delta: bool,
    cancel_before_delta: bool,
    unsafe_before_delta: bool,
    attempts: int,
    probe: StreamProbe,

    implements ai.Client {
        function id(self) -> string {
            self.client_id
        }

        function render(self, input: ai.ModelTurnInput) -> baml.http.Request {
            let _ = input;
            baml.http.Request {
                method: "POST",
                url: `https://example.invalid/${self.client_id}`,
                headers: {},
                body: "{}",
            }
        }

        function invoke(self, input: ai.ModelTurnInput) -> ai.ModelTurn {
            let _ = input;
            ai.ModelTurn { calls: [],
                content: [ai.content.Text { text: self.chunks.join("") }],
                stop_reason: ai.content.StopReason.Complete,
                usage: null,
            }
        }
    }

    implements ai.stream.StreamingClient {
        function invoke_stream(self, input: ai.ModelTurnInput) -> ai.stream.TurnStream {
            let _ = input;
            self.attempts += 1;
            let attempt = self.attempts;
            self.probe.events.push(`open:${self.client_id}:${attempt}`);
            if (attempt <= self.fail_open_attempts) {
                throw ai.errors.NetworkFailure { status_code: null,
                    provider: self.client_id,
                    detail: "failed while opening",
                    raw_body: null,
                }
            }
            if (attempt <= self.fail_open_attempts + self.fail_pull_attempts) {
                let probe = self.probe;
                let client_id = self.client_id;
                return ai.stream.TurnStream.from_event_source(() -> {
                    probe.events.push(`pull-fail:${client_id}:${attempt}`);
                    throw ai.errors.NetworkFailure { status_code: null,
                        provider: client_id,
                        detail: "failed before first delta",
                        raw_body: null,
                    }
                });
            }
            if (self.cancel_before_delta) {
                return ai.stream.TurnStream.from_event_source(() -> {
                    throw baml.panics.Cancelled { message: "scripted cancellation" }
                });
            }
            if (self.unsafe_before_delta) {
                return ai.stream.TurnStream.from_event_source(() -> {
                    throw ai.errors.StepBudgetExceeded { steps: 1 }
                });
            }
            if (self.fail_after_delta) {
                let cursor = 0;
                let probe = self.probe;
                let client_id = self.client_id;
                let prefix = self.chunks.at(0) ?? "prefix";
                return ai.stream.TurnStream.from_event_source(() -> {
                    if (cursor == 0) {
                        cursor += 1;
                        ai.stream.TextDelta { text: prefix }
                    } else {
                        probe.events.push(`partial-fail:${client_id}:${attempt}`);
                        throw ai.errors.NetworkFailure { status_code: null,
                            provider: client_id,
                            detail: "failed after first delta",
                            raw_body: prefix,
                        }
                    }
                });
            }
            ai.stream.TurnStream.from_chunks(self.chunks)
        }
    }
}

class NonStreamingClient {
    client_id: string,
    probe: StreamProbe,

    implements ai.Client {
        function id(self) -> string {
            self.probe.events.push(`id:${self.client_id}`);
            self.client_id
        }

        function render(self, input: ai.ModelTurnInput) -> baml.http.Request {
            let _ = input;
            baml.http.Request {
                method: "POST",
                url: `https://example.invalid/${self.client_id}`,
                headers: {},
                body: "{}",
            }
        }

        function invoke(self, input: ai.ModelTurnInput) -> ai.ModelTurn {
            let _ = input;
            ai.ModelTurn { calls: [],
                content: [ai.content.Text { text: "not streamed" }],
                stop_reason: ai.content.StopReason.Complete,
                usage: null,
            }
        }
    }
}

class SyntheticInvokeClient {
    attempts: int,
    cancel_immediately: bool,

    implements ai.Client {
        function id(self) -> string {
            "synthetic-invoke"
        }

        function render(self, input: ai.ModelTurnInput) -> baml.http.Request {
            let _ = input;
            baml.http.Request {
                method: "POST",
                url: "https://example.invalid/synthetic-invoke",
                headers: {},
                body: "{}",
            }
        }

        function invoke(self, input: ai.ModelTurnInput) -> ai.ModelTurn {
            let _ = input;
            self.attempts += 1;
            if (self.cancel_immediately) {
                throw baml.panics.Cancelled { message: "scripted non-stream cancellation" }
            }
            throw ai.errors.NetworkFailure { status_code: null,
                provider: "synthetic-invoke",
                detail: "retryable failure",
                raw_body: null,
            }
        }
    }
}

function scripted_client(
    client_id: string,
    chunks: string[],
    probe: StreamProbe,
    fail_open_attempts: int = 0,
    fail_pull_attempts: int = 0,
    fail_after_delta: bool = false,
    cancel_before_delta: bool = false,
    unsafe_before_delta: bool = false,
) -> ScriptedStreamClient {
    ScriptedStreamClient {
        client_id: client_id,
        chunks: chunks,
        fail_open_attempts: fail_open_attempts,
        fail_pull_attempts: fail_pull_attempts,
        fail_after_delta: fail_after_delta,
        cancel_before_delta: cancel_before_delta,
        unsafe_before_delta: unsafe_before_delta,
        attempts: 0,
        probe: probe,
    }
}

function composite_input() -> ai.ModelTurnInput {
    let spec = CompositeStreamSpec@spec();
    ai.ModelTurnInput {
        prompt: spec.prompt_template,
        journal: ai.Journal { log: [] },
        toolbox: spec.tools(),
        output_type: spec.output_type(),
    }
}

function drain_streaming(client_value: ai.stream.StreamingClient) -> string {
    client_value.invoke_stream(composite_input()).final_turn().terminal_text() ?? ""
}

function failure_name(error: unknown) -> string {
    match (error) {
        let network: ai.errors.NetworkFailure => `NetworkFailure:${network.detail}`,
        let unsafe: ai.errors.StepBudgetExceeded => `StepBudgetExceeded:${unsafe.steps}`,
        let unsupported: ai.errors.StreamingUnsupported => {
            `StreamingUnsupported:${unsupported.client_id}`
        },
        let cancelled: baml.panics.Cancelled => `Cancelled:${cancelled.message}`,
        _ => `other:${error.to_string()}`,
    }
}

function retry_stream_success_case() -> string {
    let probe = StreamProbe { events: [] };
    let inner = scripted_client("success", ["hel", "lo"], probe);
    let retry = ai.clients.Retry.new(
        inner,
        max_attempts = 3,
        backoff = ai.clients.Backoff.new(initial_ms = 0, max_ms = 0),
    );
    `${drain_streaming(retry)}|${probe.events.join(",")}`
}

test "retry_stream_success" {
    assert.equal(retry_stream_success_case(), "hello|open:success:1")
}

function retry_stream_retries_before_first_delta_case() -> string {
    let probe = StreamProbe { events: [] };
    let inner = scripted_client("flaky", ["ok"], probe, fail_pull_attempts = 1);
    let retry = ai.clients.Retry.new(
        inner,
        max_attempts = 3,
        backoff = ai.clients.Backoff.new(initial_ms = 0, max_ms = 0),
    );
    `${drain_streaming(retry)}|${probe.events.join(",")}`
}

test "retry_stream_retries_before_first_delta" {
    assert.equal(
        retry_stream_retries_before_first_delta_case(),
        "ok|open:flaky:1,pull-fail:flaky:1,open:flaky:2",
    )
}

function retry_stream_retries_when_open_fails_case() -> string {
    let probe = StreamProbe { events: [] };
    let inner = scripted_client("flaky-open", ["ok"], probe, fail_open_attempts = 1);
    let retry = ai.clients.Retry.new(
        inner,
        max_attempts = 2,
        backoff = ai.clients.Backoff.new(initial_ms = 0, max_ms = 0),
    );
    `${drain_streaming(retry)}|${probe.events.join(",")}`
}

test "retry_stream_retries_when_open_fails" {
    assert.equal(
        retry_stream_retries_when_open_fails_case(),
        "ok|open:flaky-open:1,open:flaky-open:2",
    )
}

function retry_stream_does_not_retry_after_delta_case() -> string {
    let probe = StreamProbe { events: [] };
    let inner = scripted_client("partial", ["prefix"], probe, fail_after_delta = true);
    let retry = ai.clients.Retry.new(
        inner,
        max_attempts = 3,
        backoff = ai.clients.Backoff.new(initial_ms = 0, max_ms = 0),
    );
    let stream = retry.invoke_stream(composite_input());
    let first = match (stream.next()) {
        let batch: string[] => batch.join(""),
        let done: ai.stream.Done => "done",
    };
    let second = stream.next() catch_all (e) {
        _ => failure_name(e),
    };
    `${first}|${second.to_string()}|${probe.events.join(",")}`
}

test "retry_stream_does_not_retry_after_delta" {
    assert.equal(
        retry_stream_does_not_retry_after_delta_case(),
        "prefix|NetworkFailure:failed after first delta|open:partial:1,partial-fail:partial:1",
    )
}

function fallback_stream_advances_before_first_delta_case() -> string {
    let probe = StreamProbe { events: [] };
    let first = scripted_client("first", ["unused"], probe, fail_pull_attempts = 100);
    let second = scripted_client("second", ["winner"], probe);
    let fallback = ai.clients.Fallback { members: [first, second] };
    `${drain_streaming(fallback)}|${probe.events.join(",")}`
}

test "fallback_stream_advances_before_first_delta_in_order" {
    assert.equal(
        fallback_stream_advances_before_first_delta_case(),
        "winner|open:first:1,pull-fail:first:1,open:second:1",
    )
}

function fallback_stream_does_not_advance_after_delta_case() -> string {
    let probe = StreamProbe { events: [] };
    let first = scripted_client("first", ["prefix"], probe, fail_after_delta = true);
    let second = scripted_client("second", ["winner"], probe);
    let fallback = ai.clients.Fallback { members: [first, second] };
    let stream = fallback.invoke_stream(composite_input());
    let first_delta = match (stream.next()) {
        let batch: string[] => batch.join(""),
        let done: ai.stream.Done => "done",
    };
    let outcome = stream.next() catch_all (e) {
        _ => failure_name(e),
    };
    `${first_delta}|${outcome.to_string()}|${probe.events.join(",")}`
}

test "fallback_stream_does_not_advance_after_delta" {
    assert.equal(
        fallback_stream_does_not_advance_after_delta_case(),
        "prefix|NetworkFailure:failed after first delta|open:first:1,partial-fail:first:1",
    )
}

function round_robin_stream_rotates_once_per_stream_case() -> string {
    let probe = StreamProbe { events: [] };
    let first = scripted_client("first", ["a"], probe);
    let second = scripted_client("second", ["b"], probe);
    let round_robin = ai.clients.RoundRobin.new([first, second]);
    let values = [drain_streaming(round_robin), drain_streaming(round_robin), drain_streaming(round_robin)];
    `${values.join(",")}|${probe.events.join(",")}`
}

test "round_robin_stream_rotates_once_per_stream" {
    assert.equal(
        round_robin_stream_rotates_once_per_stream_case(),
        "a,b,a|open:first:1,open:second:1,open:first:2",
    )
}

function streaming_composites_enforce_member_capability_case() -> string {
    let fallback_probe = StreamProbe { events: [] };
    let nonstream = NonStreamingClient { client_id: "plain", probe: fallback_probe };
    let streaming = scripted_client("streaming", ["ok"], fallback_probe);
    let fallback = ai.clients.Fallback { members: [nonstream, streaming] };
    let fallback_value = drain_streaming(fallback);

    let retry_probe = StreamProbe { events: [] };
    let retry_nonstream = NonStreamingClient { client_id: "retry-plain", probe: retry_probe };
    let retry = ai.clients.Retry.new(
        retry_nonstream,
        max_attempts = 3,
        backoff = ai.clients.Backoff.new(initial_ms = 0, max_ms = 0),
    );
    let retry_error = drain_streaming(retry) catch_all (e) {
        _ => failure_name(e),
    };

    let rr_probe = StreamProbe { events: [] };
    let rr_nonstream = NonStreamingClient { client_id: "rr-plain", probe: rr_probe };
    let rr_streaming = scripted_client("rr-streaming", ["next"], rr_probe);
    let round_robin = ai.clients.RoundRobin.new([rr_nonstream, rr_streaming]);
    let rr_error = drain_streaming(round_robin) catch_all (e) {
        _ => failure_name(e),
    };
    let rr_value = drain_streaming(round_robin);

    [
        `fallback=${fallback_value}:${fallback_probe.events.join(",")}`,
        `retry=${retry_error}:${retry_probe.events.join(",")}`,
        `round_robin=${rr_error},${rr_value}:${rr_probe.events.join(",")}`,
    ]
        .join("|")
}

test "streaming_composites_enforce_member_capability" {
    assert.equal(
        streaming_composites_enforce_member_capability_case(),
        "fallback=ok:id:plain,open:streaming:1|retry=StreamingUnsupported:retry-plain:id:retry-plain|round_robin=StreamingUnsupported:rr-plain,next:id:rr-plain,open:rr-streaming:1",
    )
}

function streaming_composites_nest_as_capabilities_case() -> string {
    let probe = StreamProbe { events: [] };
    let first = scripted_client("first", ["unused"], probe, fail_pull_attempts = 100);
    let second = scripted_client("second", ["nested"], probe);
    let fallback = ai.clients.Fallback { members: [first, second] };
    let retry = ai.clients.Retry.new(
        fallback,
        max_attempts = 2,
        backoff = ai.clients.Backoff.new(initial_ms = 0, max_ms = 0),
    );
    `${drain_streaming(retry)}|${probe.events.join(",")}`
}

test "streaming_composites_nest_as_capabilities" {
    assert.equal(
        streaming_composites_nest_as_capabilities_case(),
        "nested|open:first:1,pull-fail:first:1,open:second:1",
    )
}

function streaming_composites_propagate_cancellation_case() -> string {
    let probe = StreamProbe { events: [] };
    let cancelled = scripted_client("cancelled", ["unused"], probe, cancel_before_delta = true);
    let backup = scripted_client("backup", ["must-not-open"], probe);
    let fallback = ai.clients.Fallback { members: [cancelled, backup] };
    let outcome = drain_streaming(fallback) catch (e) {
        let cancelled: baml.panics.Cancelled => failure_name(cancelled),
    };
    `${outcome}|${probe.events.join(",")}`
}

test "streaming_composites_propagate_cancellation" {
    assert.equal(
        streaming_composites_propagate_cancellation_case(),
        "Cancelled:scripted cancellation|open:cancelled:1",
    )
}

function streaming_composites_do_not_retry_unsafe_failure_case() -> string {
    let probe = StreamProbe { events: [] };
    let unsafe = scripted_client("unsafe", ["unused"], probe, unsafe_before_delta = true);
    let retry = ai.clients.Retry.new(
        unsafe,
        max_attempts = 3,
        backoff = ai.clients.Backoff.new(initial_ms = 0, max_ms = 0),
    );
    let outcome = drain_streaming(retry) catch_all (e) {
        _ => failure_name(e),
    };
    `${outcome}|${probe.events.join(",")}`
}

test "streaming_composites_do_not_retry_unsafe_failure" {
    assert.equal(
        streaming_composites_do_not_retry_unsafe_failure_case(),
        "StepBudgetExceeded:1|open:unsafe:1",
    )
}

function nonstream_composites_preserve_cancellation_case() -> string {
    let retry_inner = SyntheticInvokeClient { attempts: 0, cancel_immediately: true };
    let retry = ai.clients.Retry.new(
        retry_inner,
        max_attempts = 3,
        backoff = ai.clients.Backoff.new(initial_ms = 0, max_ms = 0),
    );
    let retry_outcome = retry.invoke(composite_input()) catch_all (e) {
        let cancelled: baml.panics.Cancelled => "cancelled",
        _ => "other",
    };

    let round_robin_inner = SyntheticInvokeClient { attempts: 0, cancel_immediately: true };
    let round_robin = ai.clients.RoundRobin.new([round_robin_inner]);
    let round_robin_outcome = round_robin.invoke(composite_input()) catch_all (e) {
        let cancelled: baml.panics.Cancelled => "cancelled",
        _ => "other",
    };

    let fallback_inner = SyntheticInvokeClient { attempts: 0, cancel_immediately: true };
    let fallback_backup = SyntheticInvokeClient { attempts: 0, cancel_immediately: false };
    let fallback = ai.clients.Fallback { members: [fallback_inner, fallback_backup] };
    let fallback_outcome = fallback.invoke(composite_input()) catch_all (e) {
        let cancelled: baml.panics.Cancelled => "cancelled",
        _ => "other",
    };

    `retry=${retry_outcome}:${retry_inner.attempts}|round_robin=${round_robin_outcome}:${round_robin_inner.attempts}|fallback=${fallback_outcome}:${fallback_inner.attempts}:${fallback_backup.attempts}`
}

test "nonstream_composites_preserve_typed_cancellation" {
    assert.equal(
        nonstream_composites_preserve_cancellation_case(),
        "retry=cancelled:1|round_robin=cancelled:1|fallback=cancelled:1:0",
    )
}
