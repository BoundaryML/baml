// Part 2 wire-record regressions: the selected LLMCall rides the journal to
// the listener per attempt (test 6), and retry wrappers record failed legs
// with `selected: false` prepended oldest-first (test 7).

class WireClient {
    implements ai.Client {
        function id(self) -> string {
            "wire-scripted"
        }

        function render(self, input: ai.ModelTurnInput) -> baml.http.Request {
            let _ = input;
            baml.http.Request { method: "POST", url: "https://example.invalid/w", headers: {}, body: "{}" }
        }

        function invoke(self, input: ai.ModelTurnInput) -> ai.ModelTurn {
            let _ = input;
            ai.ModelTurn {
                calls: [
                    ai.events.LLMCall {
                        client_name: "wire-scripted",
                        provider: "scripted",
                        timing: ai.events.Timing { start_time_utc_ms: 1000n, duration_ms: 42n, ttft_ms: null },
                        http_request: null,
                        http_response: ai.events.HttpResponse { status: 200, headers: {}, body: null },
                        usage: null,
                        selected: true,
                        sse_responses: null,
                    },
                ],
                content: [ai.content.Text { text: "{\"message\": \"hi\"}" }],
                stop_reason: ai.content.StopReason.Complete,
                usage: null,
            }
        }
    }
}

class FlakyClient {
    fails_left: int,

    implements ai.Client {
        function id(self) -> string {
            "flaky"
        }

        function render(self, input: ai.ModelTurnInput) -> baml.http.Request {
            let _ = input;
            baml.http.Request { method: "POST", url: "https://example.invalid/f", headers: {}, body: "{}" }
        }

        function invoke(self, input: ai.ModelTurnInput) -> ai.ModelTurn {
            let _ = input;
            if (self.fails_left > 0) {
                self.fails_left -= 1;
                throw ai.errors.classify_http("flaky", 503, "unavailable")
            }
            ai.ModelTurn {
                calls: [],
                content: [ai.content.Text { text: "{\"message\": \"ok\"}" }],
                stop_reason: ai.content.StopReason.Complete,
                usage: null,
            }
        }
    }
}

test "selected wire record is delivered after AssistantMessage" {
    let probe = Probe { labels: [] };
    let cb = (e: ai.events.Event) -> void {
        match (e) {
            let c: ai.events.LLMCall => {
                probe.labels.push(`LLMCall:${c.provider}:${if (c.selected) { "sel" } else { "leg" }}`);
            },
            _ => {
                probe.labels.push(label_of(e));
            },
        };
    };
    let g = Greet("world", client = WireClient {  }, on_event = cb);
    assert.equal(g.message, "hi");
    assert.equal(probe.labels, ["RunStarted", "AssistantMessage", "LLMCall:scripted:sel", "FinalProduced"])
}

test "retry failed legs are recorded with status and prepended" {
    let probe = Probe { labels: [] };
    let cb = (e: ai.events.Event) -> void {
        match (e) {
            let c: ai.events.LLMCall => {
                let status = c.http_response?.status ?? -1;
                probe.labels.push(`LLMCall:${status}:${if (c.selected) { "sel" } else { "leg" }}`);
            },
            _ => {
                probe.labels.push(label_of(e));
            },
        };
    };
    let retry = ai.clients.Retry.new(inner = FlakyClient { fails_left: 2 }, max_attempts = 3);
    let g = Greet("world", client = retry, on_event = cb);
    assert.equal(g.message, "ok");
    assert.equal(
        probe.labels,
        ["RunStarted", "AssistantMessage", "LLMCall:503:leg", "LLMCall:503:leg", "FinalProduced"],
    )
}
