// v0 oracle: engine/baml-runtime/src/internal/llm_client/orchestrator/{call,stream}.rs.
// A successful provider response stops fallback before SAP/finish validation.
// Provider failures advance even after streamed partials (a known v1 gap).
// All sockets and providers below are local; reliability executes in BAML.

function FallbackInteger() -> int {
    client: "openai/gpt-4o-mini"
    prompt: `Return an integer. ${ctx.output_format()}`
}

function fallback_body(text: string = "winner", incomplete: bool = false) -> string {
    let status = if (incomplete) {
        "incomplete"
    } else {
        "completed"
    };
    let details = if (incomplete) {
        `{"reason":"max_output_tokens"}`
    } else {
        "null"
    };
    `{"id":"resp_offline","object":"response","model":"offline","status":"${status}","incomplete_details":${details},"output":[{"type":"message","content":[{"type":"output_text","text":${baml.json.stringify(baml.json.to_json(text))}}]}],"usage":{"input_tokens":3,"output_tokens":1,"total_tokens":4}}`
}

function fallback_success_events(text: string) -> string[] {
    [
        root.llm_mock.sse_data(
            `{"type":"response.output_text.delta","item_id":"msg_1","output_index":0,"content_index":0,"sequence_number":0,"delta":${baml.json.stringify(baml.json.to_json(text))}}`,
        ),
        root.llm_mock.sse_data(
            `{"type":"response.completed","sequence_number":1,"response":${fallback_body(text = text)}}`,
        ),
    ]
}

function fallback_leaf(url: string) -> openai.ResponsesClient {
    openai.ResponsesClient.new(model = "offline", api_key = "test-key", base_url = url)
}

function fallback_error(error: unknown) -> string {
    match (error) {
        ai.errors.NetworkFailure => "NetworkFailure",
        ai.errors.InvalidRequest => "InvalidRequest",
        ai.errors.ParseFailed => "ParseFailed",
        baml.errors.LlmClient => "ParseFailed",
        ai.errors.FinishReasonError => "FinishReasonError",
        ai.errors.RateLimited => "RateLimited",
        _ => `unexpected:${error.to_string()}`,
    }
}

function fallback_value(chain: ai.clients.Fallback, streaming: bool) -> string {
    if (streaming) {
        ai.stream.from_spec<string>(CompositeStreamSpec@spec(), client = chain).final()
    } else {
        ai.Agent.new(client = chain).run(CompositeStreamSpec@spec()).value
    }
}

function fallback_http_case(status: int, streaming: bool) -> string {
    root.llm_mock.mock_json_serve([root.llm_mock.MockResponse.error(status, `{"error":{"message":"primary failed"}}`)], (
        primary: root.llm_mock.MockJsonServer,
    ) -> {
        root.llm_mock.mock_sse_serve(fallback_success_events("winner"), (
            stream_backup: root.llm_mock.MockSseServer,
        ) -> {
            root.llm_mock.mock_json_serve([root.llm_mock.MockResponse.ok(fallback_body())], (
                backup: root.llm_mock.MockJsonServer,
            ) -> {
                let backup_url = if (streaming) {
                    stream_backup.base_url()
                } else {
                    backup.base_url()
                };
                let chain = ai.clients.Fallback {
                    members: [fallback_leaf(primary.base_url()), fallback_leaf(backup_url)],
                };
                let value = fallback_value(chain, streaming);
                `${value}|${primary.request_count()}|${backup.request_count()}|${stream_backup.request_count()}`
            })
        })
    })
}

function fallback_http_matrix(streaming: bool) -> bool {
    let expected = if (streaming) {
        "winner|1|0|1"
    } else {
        "winner|1|1|0"
    };
    for (let status in [400, 401, 403, 408, 429, 500, 503]) {
        assert.equal(fallback_http_case(status, streaming), expected);
    }
    true
}

test "fallback_request_failures_plain" {
    assert.is_true(fallback_http_matrix(false))
}

test "fallback_request_failures_streaming" {
    assert.is_true(fallback_http_matrix(true))
}

function fallback_connection_case(streaming: bool) -> string {
    // TCP port zero is not a listening service. No DNS or external endpoint.
    root.llm_mock.mock_json_serve([root.llm_mock.MockResponse.ok(fallback_body())], (
        backup: root.llm_mock.MockJsonServer,
    ) -> {
        root.llm_mock.mock_sse_serve(fallback_success_events("winner"), (
            stream_backup: root.llm_mock.MockSseServer,
        ) -> {
            let url = if (streaming) {
                stream_backup.base_url()
            } else {
                backup.base_url()
            };
            let chain = ai.clients.Fallback {
                members: [fallback_leaf("http://127.0.0.1:0"), fallback_leaf(url)],
            };
            `${fallback_value(chain, streaming)}|${backup.request_count()}|${stream_backup.request_count()}`
        })
    })
}

test "fallback_connection_failure_plain" {
    assert.equal(fallback_connection_case(false), "winner|1|0")
}

test "fallback_connection_failure_streaming" {
    assert.equal(fallback_connection_case(true), "winner|0|1")
}

function fallback_json_case(body: string, integer: bool = false) -> string {
    root.llm_mock.mock_json_serve([root.llm_mock.MockResponse.ok(body)], (
        primary: root.llm_mock.MockJsonServer,
    ) -> {
        root.llm_mock.mock_json_serve([root.llm_mock.MockResponse.ok(fallback_body(text = "42"))], (
            backup: root.llm_mock.MockJsonServer,
        ) -> {
            let chain = ai.clients.Fallback {
                members: [fallback_leaf(primary.base_url()), fallback_leaf(backup.base_url())],
            };
            let value = if (integer) {
                ai.Agent.new(client = chain).run(FallbackInteger@spec()).value.to_string()
            } else {
                fallback_value(chain, false)
            } catch_all (e) {
                _ => fallback_error(e)
            };
            `${value}|${primary.request_count()}|${backup.request_count()}`
        })
    })
}

test "fallback_primary_success_does_not_call_backup" {
    assert.equal(fallback_json_case(fallback_body()), "winner|1|0")
}

test "fallback_malformed_response_advances" {
    assert.equal(fallback_json_case("{broken"), "42|1|1")
}

function fallback_default_schema_gap_case() -> bool {
    let actual = fallback_json_case(fallback_body(text = "not an integer"), integer = true);
    assert.equal(actual, "ParseFailed|2|0");
    if (actual != "ParseFailed|1|0") {
        throw baml.panics.AssertionFailed { message: "v0 fallback parity: ParseFailed|1|0" };
    }
    true
}

test "fallback_output_parse_failure_no_replay_v0_parity" with fallback_known_gap("ParseFailed|1|0") {
    assert.is_true(fallback_default_schema_gap_case())
}

test "fallback_finish_reason_failure_does_not_advance" {
    assert.equal(
        fallback_json_case(fallback_body(text = "prefix", incomplete = true)),
        "FinishReasonError|1|0",
    )
}

function fallback_stream_case(events: string[], integer: bool = false) -> string {
    root.llm_mock.mock_sse_serve(events, (primary: root.llm_mock.MockSseServer) -> {
        root.llm_mock.mock_sse_serve(fallback_success_events("42"), (
            backup: root.llm_mock.MockSseServer,
        ) -> {
            let chain = ai.clients.Fallback {
                members: [fallback_leaf(primary.base_url()), fallback_leaf(backup.base_url())],
            };
            let value = if (integer) {
                ai.stream.from_spec<int>(FallbackInteger@spec(), client = chain).final().to_string()
            } else {
                fallback_value(chain, true)
            } catch_all (e) {
                _ => fallback_error(e)
            };
            `${value}|${primary.request_count()}|${backup.request_count()}`
        })
    })
}

function fallback_failed_event() -> string {
    root.llm_mock.sse_data(
        `{"type":"response.failed","sequence_number":1,"response":{"id":"resp_offline","object":"response","model":"offline","output":[],"status":"failed","error":{"code":"server_error","message":"primary failed"}}}`,
    )
}

function fallback_prefix_event() -> string {
    root.llm_mock.sse_data(
        `{"type":"response.output_text.delta","item_id":"msg_1","output_index":0,"content_index":0,"sequence_number":0,"delta":"prefix"}`,
    )
}

test "fallback_stream_provider_error_before_text" {
    assert.equal(fallback_stream_case([fallback_failed_event()]), "42|1|1")
}

test "fallback_stream_malformed_event_before_text" {
    assert.equal(fallback_stream_case([root.llm_mock.sse_data("{broken")]), "42|1|1")
}

test "fallback_stream_eof_before_text" {
    assert.equal(fallback_stream_case([]), "42|1|1")
}

test "fallback_stream_output_parse_failure_does_not_advance" {
    assert.equal(
        fallback_stream_case(fallback_success_events("not an integer"), integer = true),
        "ParseFailed|1|0",
    )
}

function fallback_stream_finish_reason_gap_case() -> bool {
    let actual = fallback_stream_case(
        [
            fallback_prefix_event(),
            root.llm_mock.sse_data(
                `{"type":"response.incomplete","sequence_number":1,"response":${fallback_body(text = "prefix", incomplete = true)}}`,
            ),
        ],
    );
    assert.equal(actual, "prefix|1|0");
    if (actual != "FinishReasonError|1|0") {
        throw baml.panics.AssertionFailed { message: "v0 fallback parity: FinishReasonError|1|0" };
    }
    true
}

test "fallback_stream_finish_reason_failure_v0_parity" with fallback_known_gap("FinishReasonError|1|0") {
    assert.is_true(fallback_stream_finish_reason_gap_case())
}

// Strict expected-failure runner for a confirmed v0 compatibility gap. The
// body checks observed behavior before the single parity assertion. Only that
// assertion's specific message is allowed; unrelated failures/errors fail CI.
function fallback_known_gap(expected_value: string) -> testing.TestRunner {
    (run_once) -> testing.TestReportThunk {
        () -> testing.TestReport {
            let report = run_once();
            let run = report.runs.at(0);
            let expected = report.outcome == "fail"
                && report.runs.length() == 1
                && (run?.message ?? "") == `v0 fallback parity: ${expected_value}`;
            testing.TestReport {
                outcome: if (expected) {
                    "pass"
                } else {
                    "fail"
                },
                runs: report.runs,
            }
        }
    }
}

// The producer cannot fail until the consumer acknowledges a visible prefix.
// This is deterministic even when CI delays/coalesces socket reads.
class FallbackStreamGate {
    released: bool,
    requests: int,
    tail: string[],
    function handle(self, req: baml.http.Request) -> baml.http.Response {
        let _ = req;
        self.requests += 1;
        let response = baml.http.Response.new_streaming(200, { "content-type": "text/event-stream" });
        spawn {
            response.write((fallback_prefix_event() + "\n\n").to_utf8());
            while (!self.released) {
                baml.sys.sleep(baml.time.Duration.from_milliseconds(1));
            }
            for (let event in self.tail) {
                response.write((event + "\n\n").to_utf8());
            }
            response.end();
            null
        };
        response
    }
}

function fallback_stream_after_text_case(tail: string[]) -> string {
    let gate = FallbackStreamGate { released: false, requests: 0, tail: tail };
    let server = baml.http.Server.bind("127.0.0.1:0");
    let task = spawn { server.serve(gate.handle) };
    defer { gate.released = true; task.cancel(); }
    root.llm_mock.mock_sse_serve(fallback_success_events("42"), (
        backup: root.llm_mock.MockSseServer,
    ) -> {
        let chain = ai.clients.Fallback {
            members: [fallback_leaf(`http://${server.addr}`), fallback_leaf(backup.base_url())],
        };
        let stream = ai.stream.from_spec<string>(CompositeStreamSpec@spec(), client = chain);
        assert.equal(stream.next(), "prefix");
        gate.released = true;
        let value = stream.final() catch_all (e) {
            _ => fallback_error(e)
        };
        `${value}|${gate.requests}|${backup.request_count()}`
    })
}

function fallback_stream_gap_case(tail: string[], expected_value: string = "42|1|1") -> bool {
    let actual = fallback_stream_after_text_case(tail);
    // A fix should remove this runner and the current-behavior assertion.
    assert.equal(actual, "NetworkFailure|1|0");
    if (actual != expected_value) {
        throw baml.panics.AssertionFailed { message: `v0 fallback parity: ${expected_value}` };
    }
    true
}

test "fallback_stream_provider_error_after_text_v0_parity" with fallback_known_gap("42|1|1") {
    assert.is_true(fallback_stream_gap_case([fallback_failed_event()]))
}

test "fallback_stream_malformed_event_after_text_v0_parity" with fallback_known_gap("42|1|1") {
    assert.is_true(fallback_stream_gap_case([root.llm_mock.sse_data("{broken")]))
}

// v0 accepts natural EOF after text even without a provider terminal event.
// This differs from an actual transport/provider error after that text.
test "fallback_stream_eof_after_text_v0_parity" with fallback_known_gap("prefix|1|0") {
    assert.is_true(fallback_stream_gap_case([], expected_value = "prefix|1|0"))
}

function fallback_exhaustion_case(streaming: bool, singleton: bool = false) -> string {
    root.llm_mock.mock_json_serve([root.llm_mock.MockResponse.error(503, "first")], (
        primary: root.llm_mock.MockJsonServer,
    ) -> {
        root.llm_mock.mock_json_serve([root.llm_mock.MockResponse.error(401, "last")], (
            backup: root.llm_mock.MockJsonServer,
        ) -> {
            let members: ai.Client[] = if (singleton) {
                [fallback_leaf(primary.base_url())]
            } else {
                [fallback_leaf(primary.base_url()), fallback_leaf(backup.base_url())]
            };
            let chain = ai.clients.Fallback { members: members };
            let value = fallback_value(chain, streaming) catch_all (e) {
                _ => fallback_error(e)
            };
            `${value}|${primary.request_count()}|${backup.request_count()}`
        })
    })
}

test "fallback_exhaustion_plain_last_error" {
    assert.equal(fallback_exhaustion_case(false), "InvalidRequest|1|1")
}

test "fallback_exhaustion_stream_last_error" {
    assert.equal(fallback_exhaustion_case(true), "InvalidRequest|1|1")
}

test "fallback_singleton_plain" {
    assert.equal(fallback_exhaustion_case(false, singleton = true), "NetworkFailure|1|0")
}

test "fallback_singleton_stream" {
    assert.equal(fallback_exhaustion_case(true, singleton = true), "NetworkFailure|1|0")
}

function fallback_empty_case(streaming: bool) -> string {
    fallback_value(ai.clients.Fallback { members: [] }, streaming) catch_all (e) {
        _ => fallback_error(e)
    }
}

test "fallback_empty_plain" {
    assert.equal(fallback_empty_case(false), "InvalidRequest")
}

test "fallback_empty_stream" {
    assert.equal(fallback_empty_case(true), "InvalidRequest")
}

class FallbackInvokeProbe {
    client_id: string,
    failures: int,
    attempts: int,
    probe: StreamProbe,
    implements ai.Client {
        function id(self) -> string {
            self.client_id
        }
        function render(self, input: ai.ModelTurnInput) -> baml.http.Request {
            let _ = input;
            baml.http.Request { method: "POST", url: "http://127.0.0.1:0", headers: {}, body: "{}" }
        }
        function invoke(self, input: ai.ModelTurnInput) -> ai.ModelTurn {
            let _ = input;
            self.attempts += 1;
            self.probe.events.push(`open:${self.client_id}:${self.attempts}`);
            if (self.attempts <= self.failures) {
                throw ai.errors.NetworkFailure { provider: self.client_id, detail: self.client_id, status_code: null, raw_body: null }
            }
            ai.ModelTurn {
                calls: [],
                content: [ai.content.Text { text: self.client_id }],
                stop_reason: ai.content.StopReason.Complete,
                usage: null,
            }
        }
    }
}

function fallback_probe_leaf(
    name: string,
    failures: int,
    streaming: bool,
    probe: StreamProbe,
) -> ai.Client {
    if (streaming) {
        scripted_client(name, [name], probe, fail_open_attempts = failures)
    } else {
        FallbackInvokeProbe { client_id: name, failures: failures, attempts: 0, probe: probe }
    }
}

function fallback_probe_value(value: ai.Client, streaming: bool) -> string {
    if (streaming) {
        match (value) {
            let capable: ai.stream.StreamingClient => drain_streaming(capable),
            _ => baml.sys.panic("probe lost streaming capability"),
        }
    } else {
        value.invoke(composite_input()).terminal_text() ?? ""
    }
}

function fallback_policy_case(mode: string, streaming: bool) -> string {
    let probe = StreamProbe { events: [] };
    let a = fallback_probe_leaf("a", 100, streaming, probe);
    let b = fallback_probe_leaf(
        "b",
        if (mode == "outer_retry") {
            1
        } else {
            100
        },
        streaming,
        probe,
    );
    let c = fallback_probe_leaf("c", 0, streaming, probe);
    let unused = fallback_probe_leaf("unused", 0, streaming, probe);
    let backoff = ai.clients.Backoff.new(initial_ms = 0, max_ms = 0);
    let value: ai.Client = match (mode) {
        "nested" => {
            ai.clients.Fallback { members: [ai.clients.Fallback { members: [a, b] }, c, unused] }
        },
        "inner_retry" => {
            ai.clients.Fallback {
                members: [ai.clients.Retry.new(a, max_attempts = 2, backoff = backoff), c, unused],
            }
        },
        "outer_retry" => {
            ai.clients.Retry.new(
                ai.clients.Fallback { members: [a, b] },
                max_attempts = 2,
                backoff = backoff,
            )
        },
        "reuse" => ai.clients.Fallback { members: [a, c, unused] },
        _ => ai.clients.Fallback { members: [a, b, c, unused] },
    };
    let first = fallback_probe_value(value, streaming);
    let result = if (mode == "reuse") {
        `${first},${fallback_probe_value(value, streaming)}`
    } else {
        first
    };
    `${result}|${probe.events.join(",")}`
}

function fallback_policy_matrix(streaming: bool) -> bool {
    assert.equal(fallback_policy_case("linear", streaming), "c|open:a:1,open:b:1,open:c:1");
    assert.equal(fallback_policy_case("nested", streaming), "c|open:a:1,open:b:1,open:c:1");
    assert.equal(fallback_policy_case("inner_retry", streaming), "c|open:a:1,open:a:2,open:c:1");
    assert.equal(
        fallback_policy_case("outer_retry", streaming),
        "b|open:a:1,open:b:1,open:a:2,open:b:2",
    );
    assert.equal(
        fallback_policy_case("reuse", streaming),
        "c,c|open:a:1,open:c:1,open:a:2,open:c:2",
    );
    true
}

test "fallback_order_nesting_retries_and_reuse_plain" {
    assert.is_true(fallback_policy_matrix(false))
}

test "fallback_order_nesting_retries_and_reuse_streaming" {
    assert.is_true(fallback_policy_matrix(true))
}

function fallback_timeout_case(streaming: bool, first_token: bool) -> string {
    root.llm_mock.mock_json_serve(
        [root.llm_mock.MockResponse.ok(fallback_body())],
        (slow_json: root.llm_mock.MockJsonServer) -> {
            root.llm_mock.mock_sse_serve(
                fallback_success_events("unused"),
                (slow_sse: root.llm_mock.MockSseServer) -> {
                    root.llm_mock.mock_json_serve([root.llm_mock.MockResponse.ok(fallback_body())], (
                        backup_json: root.llm_mock.MockJsonServer,
                    ) -> {
                        root.llm_mock.mock_sse_serve(fallback_success_events("winner"), (
                            backup_sse: root.llm_mock.MockSseServer,
                        ) -> {
                            let primary = openai.ResponsesClient.new(
                                model = "offline",
                                api_key = "test-key",
                                base_url = if (streaming) {
                                    slow_sse.base_url()
                                } else {
                                    slow_json.base_url()
                                },
                                request_timeout_ms = if (first_token) {
                                    3000
                                } else {
                                    100
                                },
                                time_to_first_token_timeout_ms = if (first_token) {
                                    100
                                } else {
                                    null
                                },
                            );
                            let backup = fallback_leaf(
                                if (streaming) {
                                    backup_sse.base_url()
                                } else {
                                    backup_json.base_url()
                                },
                            );
                            let chain = ai.clients.Fallback { members: [primary, backup] };
                            let actual = fallback_value(chain, streaming) catch_all (e) {
                                baml.errors.Timeout => "Timeout",
                                _ => fallback_error(e),
                            };
                            // Under executor load a request deadline can expire before the
                            // server receives anything. Do not require that dispatch wins
                            // the race; forbid primary replay and require exact backup use.
                            let primary_requests = slow_json.request_count() + slow_sse.request_count();
                            if (first_token) {
                                assert.equal(primary_requests, 1);
                            } else {
                                assert.is_true(primary_requests <= 1);
                            }
                            `${actual}|${backup_json.request_count() + backup_sse.request_count()}`
                        })
                    })
                },
                first_event_delay_ms = 1000,
            )
        },
        response_delay_ms = 1000,
    )
}

function fallback_timeout_gap_case(streaming: bool, first_token: bool) -> bool {
    let actual = fallback_timeout_case(streaming, first_token);
    assert.equal(actual, "Timeout|0");
    if (actual != "winner|1") {
        throw baml.panics.AssertionFailed { message: "v0 fallback parity: winner|1" };
    }
    true
}

test "fallback_request_timeout_plain_v0_parity" with fallback_known_gap("winner|1") {
    assert.is_true(fallback_timeout_gap_case(false, false))
}

test "fallback_request_timeout_stream_v0_parity" with fallback_known_gap("winner|1") {
    assert.is_true(fallback_timeout_gap_case(true, false))
}

test "fallback_first_token_timeout_v0_parity" with fallback_known_gap("winner|1") {
    assert.is_true(fallback_timeout_gap_case(true, true))
}

// v0 defaults to FinishReasonFilter::All. The earlier rejection scenarios
// correspond to an explicitly denied max-token reason in the v0 client.
function fallback_default_finish_reason_gap_case() -> bool {
    let actual = fallback_json_case(fallback_body(text = "prefix", incomplete = true));
    assert.equal(actual, "FinishReasonError|1|0");
    if (actual != "prefix|1|0") {
        throw baml.panics.AssertionFailed { message: "v0 fallback parity: prefix|1|0" };
    }
    true
}

test "fallback_default_finish_reason_plain_v0_parity" with fallback_known_gap("prefix|1|0") {
    assert.is_true(fallback_default_finish_reason_gap_case())
}

test "fallback_default_finish_reason_streaming" {
    assert.equal(
        fallback_stream_case(
            [
                fallback_prefix_event(),
                root.llm_mock.sse_data(
                    `{"type":"response.incomplete","sequence_number":1,"response":${fallback_body(text = "prefix", incomplete = true)}}`,
                ),
            ],
        ),
        "prefix|1|0",
    )
}

function fallback_cross_provider_case() -> string {
    root.llm_mock.mock_json_serve([root.llm_mock.MockResponse.error(503, "responses unavailable"), root.llm_mock.MockResponse.error(429, "bedrock limited"), root.llm_mock.MockResponse.ok(`{"candidates":[{"content":{"role":"model","parts":[{"text":"winner"}]},"finishReason":"STOP"}],"usageMetadata":{"promptTokenCount":3,"candidatesTokenCount":1}}`)], (
        mock: root.llm_mock.MockJsonServer,
    ) -> {
        let chain = ai.clients.Fallback {
            members: [
                fallback_leaf(mock.base_url()),
                aws.BedrockClient.new(
                    model = "offline",
                    region = "us-east-1",
                    access_key_id = "test-key",
                    secret_access_key = "test-secret",
                    endpoint_url = mock.base_url(),
                ),
                google.GeminiClient.new(
                    model = "offline",
                    api_key = "test-key",
                    base_url = mock.base_url(),
                ),
            ],
        };
        let value = fallback_value(chain, false);
        let paths: string[] = [];
        for (let request in mock.requests) {
            paths.push(request.path());
        }
        `${value}|${mock.request_count()}|${paths.join(",")}`
    })
}

test "fallback_responses_to_bedrock_to_gemini" {
    assert.equal(
        fallback_cross_provider_case(),
        "winner|3|/responses,/model/offline/converse,/models/offline:generateContent",
    )
}

test "fallback_stream_primary_success_does_not_call_backup" {
    assert.equal(fallback_stream_case(fallback_success_events("winner")), "winner|1|0")
}

test "fallback_invalid_response_envelope_advances" {
    assert.equal(fallback_json_case("[1]"), "42|1|1")
}

function fallback_unsafe_stream_case() -> string {
    let probe = StreamProbe { events: [] };
    let first = scripted_client("unsafe", ["unused"], probe, unsafe_before_delta = true);
    let second = scripted_client("backup", ["must-not-open"], probe);
    let chain = ai.clients.Fallback { members: [first, second] };
    let actual = drain_streaming(chain) catch_all (e) {
        _ => failure_name(e)
    };
    `${actual}|${probe.events.join(",")}`
}

test "fallback_unsafe_stream_failure_does_not_advance" {
    assert.equal(fallback_unsafe_stream_case(), "StepBudgetExceeded:1|open:unsafe:1")
}
