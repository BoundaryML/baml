// Part 2 wire-record regressions: the ModelCall rides the journal to the
// listener per attempt (test 6), and a retry's failed attempts record their
// calls oldest-first (test 7).

class WireClient {
    implements ai.Client {
        function requested_model(self) -> string {
            "wire-scripted"
        }
        function provider(self) -> string {
            "test"
        }

        function render(self, input: ai.ModelTurnInput) -> baml.http.Request {
            let _ = input;
            baml.http.Request { method: "POST", url: "https://example.invalid/w", headers: {}, body: "{}" }
        }

        function invoke(self, input: ai.ModelTurnInput) -> ai.ModelTurn {
            let _ = input;
            ai.ModelTurn {
                child_turns: [],
                calls: [
                    ai.events.ModelCall {
                        resolved_model: ai.events.ModelKey { provider: "scripted", model: null },
                        usage: null,
                        timing: ai.events.Timing {
                            start_time: baml.time.Instant.from_timestamp_milliseconds(1000n),
                            duration: baml.time.Duration.from_milliseconds(42n),
                            ttft: null,
                        },
                        http_request: null,
                        http_response: ai.events.HttpResponse { status: 200, headers: {}, body: null },
                        sse_responses: null,
                    },
                ],
                content: [ai.content.Text { text: "{\"message\": \"hi\"}" }],
                stop_reason: ai.content.StopReason.Complete,
            }
        }
    }
}

class FlakyClient {
    fails_left: int,

    implements ai.Client {
        function requested_model(self) -> string {
            "flaky"
        }
        function provider(self) -> string {
            "test"
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
                child_turns: [],
                calls: [],
                content: [ai.content.Text { text: "{\"message\": \"ok\"}" }],
                stop_reason: ai.content.StopReason.Complete,
            }
        }
    }
}

test "successful wire record is delivered after AssistantMessage" {
    let probe = Probe { labels: [] };
    let cb = (e: ai.events.Event) -> void {
        match (e) {
            let c: ai.events.ModelCall => {
                probe.labels.push(`ModelCall:${c.resolved_model.provider}`);
            },
            _ => {
                probe.labels.push(label_of(e));
            },
        };
    };
    let g = Greet("world", client = WireClient {  }, on_event = cb);
    assert.equal(g.message, "hi");
    assert.equal(probe.labels, ["RunStarted", "AssistantMessage", "ModelCall:scripted", "FinalProduced"])
}

test "a retry's failed attempts record their calls oldest first" {
    let probe = Probe { labels: [] };
    let cb = (e: ai.events.Event) -> void {
        match (e) {
            let c: ai.events.ModelCall => {
                let status = c.http_response?.status ?? -1;
                probe.labels.push(`ModelCall:${status}`);
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
        ["RunStarted", "AssistantMessage", "ModelCall:503", "ModelCall:503", "FinalProduced"],
    )
}
