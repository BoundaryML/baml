// Offline tests for the wrapper clients. FlakyClient is also the smallest
// possible custom client: any value implementing ai.Client works.

class FlakyClient {
    fail_first: int,
    calls: int,
    inner: ai.Client,

    implements ai.Client {
        function id(self) -> string {
            "flaky"
        }

        function invoke(self, input: ai.ModelTurnInput) -> ai.ModelTurn {
            self.calls += 1;
            if (self.calls <= self.fail_first) {
                throw ai.errors.NetworkFailure { provider: "flaky", detail: "socket dropped", raw_body: null }
            } else {
                self.inner.invoke(input)
            }
        }
    }
}

class AlwaysInvalid {
    implements ai.Client {
        function id(self) -> string {
            "invalid"
        }

        function invoke(self, input: ai.ModelTurnInput) -> ai.ModelTurn {
            throw ai.errors.InvalidRequest { provider: "invalid", status_code: 400, detail: "bad request", raw_body: null }
        }
    }
}

function final_turn_only() -> ScriptedClient {
    ScriptedClient.new(
        [
            ai.ModelTurn {
                content: [ai.Text { text: `{"flights": [], "hotels": [], "total_cost_usd": 1.0}` }],
                stop_reason: ai.StopReason.Complete,
                usage: null,
            },
        ],
    )
}

test "Retry replays Safe network failures and succeeds" {
    let flaky = FlakyClient { fail_first: 2, calls: 0, inner: final_turn_only() };
    let reliable = ai.Retry.new(
        flaky,
        max_attempts = 3,
        backoff = ai.Backoff.new(initial_ms = 1, multiplier = 1, max_ms = 1),
    );
    let spec = PlanTrip@spec(trip_request = "anywhere");
    let result = ai.Agent<Itinerary>.new(client = reliable).run(spec);
    assert.equal(result.value.total_cost_usd, 1.0);
    assert.equal(flaky.calls, 3)
}

test "Retry never resends a rejected request" {
    let reliable = ai.Retry.new(AlwaysInvalid {  }, max_attempts = 3);
    let spec = PlanTrip@spec(trip_request = "anywhere");
    let outcome = ai.Agent<Itinerary>.new(client = reliable).run(spec) catch_all (e) {
        let f: ai.errors.InvalidRequest => "rejected",
        _ => "other",
    };
    assert.equal(outcome, "rejected")
}

test "Fallback advances past a failing member" {
    let resilient = ai.Fallback { members: [AlwaysInvalid {  }, final_turn_only()] };
    let spec = PlanTrip@spec(trip_request = "anywhere");
    let result = ai.Agent<Itinerary>.new(client = resilient).run(spec);
    assert.equal(result.value.total_cost_usd, 1.0)
}

test "RoundRobin rotates members across invokes" {
    let a = final_turn_only();
    let b = final_turn_only();
    let rr = ai.RoundRobin.new([a, b]);
    let spec = PlanTrip@spec(trip_request = "anywhere");
    let first = ai.Agent<Itinerary>.new(client = rr).run(spec);
    let second = ai.Agent<Itinerary>.new(client = rr).run(spec);
    assert.equal(first.value.total_cost_usd, 1.0);
    assert.equal(second.value.total_cost_usd, 1.0);
    // one invoke landed on each member, in rotation
    assert.equal(a.received().length(), 1);
    assert.equal(b.received().length(), 1)
}
