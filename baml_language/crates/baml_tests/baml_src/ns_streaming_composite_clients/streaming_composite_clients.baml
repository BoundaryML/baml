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
    timeout_open_attempts: int,
    timeout_pull_attempts: int,
    timeout_after_delta: bool,
    attempts: int,
    probe: StreamProbe,

    implements ai.Client {
        function requested_model(self) -> string {
            self.client_id
        }
        function provider(self) -> string {
            "test"
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
                child_turns: [],
                content: [ai.content.Text { text: self.chunks.join("") }],
                stop_reason: ai.content.StopReason.Complete,
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
            if (attempt <= self.timeout_open_attempts) {
                throw baml.errors.Timeout { message: `${self.client_id} open timed out`, duration: baml.time.Duration.from_milliseconds(10), timeout_type: "timeout" }
            }
            if (attempt <= self.timeout_open_attempts + self.timeout_pull_attempts) {
                let probe = self.probe;
                let client_id = self.client_id;
                return ai.stream.TurnStream.from_event_source(() -> {
                    probe.events.push(`pull-timeout:${client_id}:${attempt}`);
                    throw baml.errors.Timeout { message: `${client_id} first event timed out`, duration: baml.time.Duration.from_milliseconds(10), timeout_type: "timeout" }
                });
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
            if (self.timeout_after_delta) {
                let cursor = 0;
                let probe = self.probe;
                let client_id = self.client_id;
                let prefix = self.chunks.at(0) ?? "prefix";
                return ai.stream.TurnStream.from_event_source(() -> {
                    if (cursor == 0) {
                        cursor += 1;
                        [ai.stream.TextDelta { text: prefix }]
                    } else {
                        probe.events.push(`partial-timeout:${client_id}:${attempt}`);
                        throw baml.errors.Timeout { message: `${client_id} stalled after first delta`, duration: baml.time.Duration.from_milliseconds(10), timeout_type: "timeout" }
                    }
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
                        [ai.stream.TextDelta { text: prefix }]
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
        function requested_model(self) -> string {
            self.probe.events.push(`id:${self.client_id}`);
            self.client_id
        }
        function provider(self) -> string {
            "test"
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
                child_turns: [],
                content: [ai.content.Text { text: "not streamed" }],
                stop_reason: ai.content.StopReason.Complete,
            }
        }
    }
}

class SyntheticInvokeClient {
    attempts: int,
    cancel_immediately: bool,

    implements ai.Client {
        function requested_model(self) -> string {
            "synthetic-invoke"
        }
        function provider(self) -> string {
            "test"
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
    timeout_open_attempts: int = 0,
    timeout_pull_attempts: int = 0,
    timeout_after_delta: bool = false,
) -> ScriptedStreamClient {
    ScriptedStreamClient {
        client_id: client_id,
        chunks: chunks,
        fail_open_attempts: fail_open_attempts,
        fail_pull_attempts: fail_pull_attempts,
        fail_after_delta: fail_after_delta,
        cancel_before_delta: cancel_before_delta,
        unsafe_before_delta: unsafe_before_delta,
        timeout_open_attempts: timeout_open_attempts,
        timeout_pull_attempts: timeout_pull_attempts,
        timeout_after_delta: timeout_after_delta,
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
            `StreamingUnsupported:${unsupported.requested_model}`
        },
        let cancelled: baml.panics.Cancelled => `Cancelled:${cancelled.message}`,
        let timeout: baml.errors.Timeout => `Timeout:${timeout.message}`,
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

// A member's own deadline before the first delta is a failed attempt, like a
// safe transport failure. After a visible delta it stays terminal, like every
// other failure, because replaying would duplicate the prefix.
function retry_stream_retries_timeout_before_first_delta_case() -> string {
    let probe = StreamProbe { events: [] };
    let inner = scripted_client("slow-open", ["ok"], probe, timeout_open_attempts = 1);
    let retry = ai.clients.Retry.new(
        inner,
        max_attempts = 2,
        backoff = ai.clients.Backoff.new(initial_ms = 0, max_ms = 0),
    );
    `${drain_streaming(retry)}|${probe.events.join(",")}`
}

test "retry_stream_retries_timeout_before_first_delta" {
    assert.equal(
        retry_stream_retries_timeout_before_first_delta_case(),
        "ok|open:slow-open:1,open:slow-open:2",
    )
}

function fallback_stream_advances_on_timeout_before_first_delta_case() -> string {
    let probe = StreamProbe { events: [] };
    let first = scripted_client("first", ["unused"], probe, timeout_pull_attempts = 100);
    let second = scripted_client("second", ["winner"], probe);
    let fallback = ai.clients.Fallback { members: [first, second] };
    `${drain_streaming(fallback)}|${probe.events.join(",")}`
}

test "fallback_stream_advances_on_timeout_before_first_delta" {
    assert.equal(
        fallback_stream_advances_on_timeout_before_first_delta_case(),
        "winner|open:first:1,pull-timeout:first:1,open:second:1",
    )
}

function fallback_stream_keeps_last_member_timeout_case() -> string {
    let probe = StreamProbe { events: [] };
    let first = scripted_client("first", ["unused"], probe, timeout_open_attempts = 100);
    let second = scripted_client("second", ["unused"], probe, timeout_pull_attempts = 100);
    let fallback = ai.clients.Fallback { members: [first, second] };
    let outcome = drain_streaming(fallback) catch_all (e) {
        _ => failure_name(e),
    };
    `${outcome}|${probe.events.join(",")}`
}

test "fallback_stream_keeps_last_member_timeout" {
    assert.equal(
        fallback_stream_keeps_last_member_timeout_case(),
        "Timeout:second first event timed out|open:first:1,open:second:1,pull-timeout:second:1",
    )
}

function fallback_stream_timeout_after_delta_is_terminal_case() -> string {
    let probe = StreamProbe { events: [] };
    let first = scripted_client("first", ["prefix"], probe, timeout_after_delta = true);
    let second = scripted_client("second", ["must-not-open"], probe);
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

test "fallback_stream_timeout_after_delta_is_terminal" {
    assert.equal(
        fallback_stream_timeout_after_delta_is_terminal_case(),
        "prefix|Timeout:first stalled after first delta|open:first:1,partial-timeout:first:1",
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
        "fallback=ok:id:plain,id:plain,open:streaming:1|retry=StreamingUnsupported:retry-plain:id:retry-plain|round_robin=StreamingUnsupported:rr-plain,next:id:rr-plain,open:rr-streaming:1",
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

// A member whose wire had several deltas ready at once: each pull of its
// stream hands out one whole batch, as an SSE read does after a backlog
// queued while the consumer was busy. With `fail_after_batches`, the pull
// after the last batch fails instead of ending the turn.
class BacklogStreamClient {
    client_id: string,
    batches: string[][],
    fail_after_batches: bool,
    attempts: int,

    implements ai.Client {
        function requested_model(self) -> string {
            self.client_id
        }
        function provider(self) -> string {
            "test"
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
                child_turns: [],
                content: [ai.content.Text { text: "not streamed" }],
                stop_reason: ai.content.StopReason.Complete,
            }
        }
    }

    implements ai.stream.StreamingClient {
        function invoke_stream(self, input: ai.ModelTurnInput) -> ai.stream.TurnStream {
            let _ = input;
            self.attempts += 1;
            let batches = self.batches;
            let fail_after_batches = self.fail_after_batches;
            let client_id = self.client_id;
            let cursor = 0;
            ai.stream.TurnStream.from_event_source(() -> {
                match (batches.at(cursor)) {
                    let batch: string[] => {
                        cursor += 1;
                        batch.map((text: string) -> ai.stream.Event {
                            ai.stream.TextDelta { text: text }
                        })
                    },
                    null => {
                        if (fail_after_batches) {
                            throw ai.errors.NetworkFailure { status_code: null,
                                provider: client_id,
                                detail: "failed after a batch",
                                raw_body: null,
                            }
                        }
                        ai.stream.Done {}
                    },
                }
            })
        }
    }
}

function backlog_client(
    client_id: string,
    batches: string[][],
    fail_after_batches: bool = false,
) -> BacklogStreamClient {
    BacklogStreamClient {
        client_id: client_id,
        batches: batches,
        fail_after_batches: fail_after_batches,
        attempts: 0,
    }
}

function no_backoff_retry(inner: ai.Client) -> ai.clients.Retry {
    ai.clients.Retry.new(
        inner,
        max_attempts = 3,
        backoff = ai.clients.Backoff.new(initial_ms = 0, max_ms = 0),
    )
}

function open_stream(client_value: ai.stream.StreamingClient) -> ai.stream.TurnStream {
    client_value.invoke_stream(composite_input())
}

// Every batch the stream returns, then one more pull to show that Done
// keeps coming once the turn is over.
function render_batches(stream: ai.stream.TurnStream) -> string {
    let out: string[] = [];
    while (true) {
        match (stream.next()) {
            let batch: string[] => {
                out.push(batch.join(","));
                null
            },
            let done: ai.stream.Done => {
                break;
            },
        };
    }
    match (stream.next()) {
        let batch: string[] => out.push(`late:${batch.join(",")}`),
        let done: ai.stream.Done => out.push("Done"),
    };
    out.join("|")
}

function composites_relay_member_batches_intact_case() -> string {
    let batches = [["a", "b", "c"], ["d"]];
    let raw = render_batches(open_stream(backlog_client("raw", batches)));
    let retry = render_batches(open_stream(no_backoff_retry(backlog_client("retry", batches))));
    let fallback = render_batches(
        open_stream(ai.clients.Fallback { members: [backlog_client("fallback", batches)] }),
    );
    let round_robin = render_batches(
        open_stream(ai.clients.RoundRobin.new([backlog_client("round-robin", batches)])),
    );
    let nested = render_batches(
        open_stream(
            no_backoff_retry(ai.clients.Fallback { members: [backlog_client("nested", batches)] }),
        ),
    );
    [`raw=${raw}`, `retry=${retry}`, `fallback=${fallback}`, `round_robin=${round_robin}`, `nested=${nested}`]
        .join(" ")
}

// A consumer that falls behind gets the member's backlog in one batch behind
// any composite, exactly as it would from the member itself, with one element
// per member delta.
test "composites_relay_member_batches_intact" {
    assert.equal(
        composites_relay_member_batches_intact_case(),
        "raw=a,b,c|d|Done retry=a,b,c|d|Done fallback=a,b,c|d|Done round_robin=a,b,c|d|Done nested=a,b,c|d|Done",
    )
}

test "composites_keep_single_delta_batches_for_a_consumer_that_keeps_up" {
    assert.equal(
        render_batches(open_stream(no_backoff_retry(backlog_client("steady", [["a"], ["b"], ["c"]])))),
        "a|b|c|Done",
    )
}

function composites_do_not_replace_after_first_batch_case() -> string {
    let retry_member = backlog_client("retry", [["a", "b"]], fail_after_batches = true);
    let retry_stream = open_stream(no_backoff_retry(retry_member));
    let retry_first = match (retry_stream.next()) {
        let batch: string[] => batch.join(","),
        let done: ai.stream.Done => "done",
    };
    let retry_second = retry_stream.next() catch_all (e) {
        _ => failure_name(e),
    };

    let first = backlog_client("first", [["a", "b"]], fail_after_batches = true);
    let second = backlog_client("second", [["must-not-open"]]);
    let fallback_stream = open_stream(ai.clients.Fallback { members: [first, second] });
    let fallback_first = match (fallback_stream.next()) {
        let batch: string[] => batch.join(","),
        let done: ai.stream.Done => "done",
    };
    let fallback_second = fallback_stream.next() catch_all (e) {
        _ => failure_name(e),
    };

    [
        `retry=${retry_first}|${retry_second.to_string()}|opened:${retry_member.attempts}`,
        `fallback=${fallback_first}|${fallback_second.to_string()}|opened:${first.attempts},${second.attempts}`,
    ]
        .join(" ")
}

// A multi-delta first batch makes the whole batch visible at once, so a
// failure after it is terminal: nothing is retried and no prefix is replayed.
test "composites_do_not_replace_the_stream_after_a_multi_delta_first_batch" {
    assert.equal(
        composites_do_not_replace_after_first_batch_case(),
        "retry=a,b|NetworkFailure:failed after a batch|opened:1 fallback=a,b|NetworkFailure:failed after a batch|opened:1,0",
    )
}

function composite_terminal_follows_relayed_batches_case() -> string {
    let probe = StreamProbe { events: [] };
    let failed = scripted_client("failed", ["unused"], probe, fail_pull_attempts = 100);
    let winner = backlog_client("winner", [["a", "b"], ["c", "d"]]);
    let stream = open_stream(ai.clients.Fallback { members: [failed, winner] });
    let batches = render_batches(stream);
    let turn = stream.final_turn();
    let children = turn.child_turns.map((child: ai.ModelTurn) -> string {
        if (child.error == null) {
            child.terminal_text() ?? ""
        } else {
            "failed"
        }
    });
    let complete = turn.stop_reason == ai.content.StopReason.Complete;
    `${batches}|terminal=${stream._saw_terminal}|text=${turn.terminal_text() ?? ""}|complete=${complete}|children=${children.join(",")}`
}

// The member's terminal events follow all of its text, and the settled turn
// carries the failed attempt and the winning member's turn.
test "composite_terminal_follows_relayed_batches" {
    assert.equal(
        composite_terminal_follows_relayed_batches_case(),
        "a,b|c,d|Done|terminal=true|text=abcd|complete=true|children=failed,abcd",
    )
}

function render_partials(stream: ai.stream.Stream<string>) -> string {
    let out: string[] = [];
    while (true) {
        match (stream.next()) {
            let partial: string => {
                out.push(partial);
                null
            },
            let done: ai.stream.Done => {
                break;
            },
        };
    }
    `${out.join(",")}|final=${stream.final()}`
}

function typed_partials_follow_member_batches_case() -> string {
    let batches = [["he", "l", "lo"]];
    let raw = render_partials(
        ai.stream.from_spec(CompositeStreamSpec@spec(), client = backlog_client("raw", batches)),
    );
    let wrapped = render_partials(
        ai.stream.from_spec(
            CompositeStreamSpec@spec(),
            client = no_backoff_retry(backlog_client("wrapped", batches)),
        ),
    );
    `raw=${raw} wrapped=${wrapped}`
}

// A typed stream parses once per batch, so behind a composite a slow consumer
// sees as many partials as it would from the member itself.
test "typed_partials_follow_member_batches_behind_a_composite" {
    assert.equal(
        typed_partials_follow_member_batches_case(),
        "raw=hello|final=hello wrapped=hello|final=hello",
    )
}
