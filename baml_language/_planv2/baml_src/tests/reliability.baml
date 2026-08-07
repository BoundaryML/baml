// Offline tests for the wrapper clients. FlakyClient is also the smallest
// possible custom client: any value implementing root.ai.Client works.

class FlakyClient {
    fail_first: int,
    calls: int,
    inner: root.ai.Client,

    implements root.ai.Client {
        function id(self) -> string {
            "flaky"
        }

        function invoke(self, input: root.ai.ModelTurnInput) -> root.ai.ModelTurn {
            self.calls += 1;
            if (self.calls <= self.fail_first) {
                throw root.ai.errors.NetworkFailure { provider: "flaky", detail: "socket dropped", raw_body: null }
            } else {
                self.inner.invoke(input)
            }
        }
    }
}

class AlwaysInvalid {
    implements root.ai.Client {
        function id(self) -> string {
            "invalid"
        }

        function invoke(self, input: root.ai.ModelTurnInput) -> root.ai.ModelTurn {
            throw root.ai.errors.InvalidRequest { provider: "invalid", status_code: 400, detail: "bad request", raw_body: null }
        }
    }
}

function final_turn_only() -> ScriptedClient {
    ScriptedClient.new(
        [
            root.ai.ModelTurn {
                content: [root.ai.Text { text: `{"flights": [], "hotels": [], "total_cost_usd": 1.0}` }],
                stop_reason: root.ai.StopReason.Complete,
                usage: null,
            },
        ],
    )
}

test "Retry replays Safe network failures and succeeds" {
    let flaky = FlakyClient { fail_first: 2, calls: 0, inner: final_turn_only() };
    let reliable = root.ai.Retry.new(
        flaky,
        max_attempts = 3,
        backoff = root.ai.Backoff.new(initial_ms = 1, multiplier = 1, max_ms = 1),
    );
    let spec = plan_trip_spec("anywhere", reliable);
    let result = root.ai.Agent<Itinerary>.new().run(spec);
    assert.equal(result.value.total_cost_usd, 1.0);
    assert.equal(flaky.calls, 3)
}

test "Retry never resends a rejected request" {
    let reliable = root.ai.Retry.new(AlwaysInvalid {  }, max_attempts = 3);
    let spec = plan_trip_spec("anywhere", reliable);
    let outcome = root.ai.Agent<Itinerary>.new().run(spec) catch_all (e) {
        let f: root.ai.errors.InvalidRequest => "rejected",
        _ => "other",
    };
    assert.equal(outcome, "rejected")
}

test "Fallback advances past a failing member" {
    let resilient = root.ai.Fallback { members: [AlwaysInvalid {  }, final_turn_only()] };
    let spec = plan_trip_spec("anywhere", resilient);
    let result = root.ai.Agent<Itinerary>.new().run(spec);
    assert.equal(result.value.total_cost_usd, 1.0)
}
