// Offline LLM regressions for the schema-correction budget on the real ai.Agent.
// InvalidSchemaClient always returns a complete turn that cannot parse as int.

function SchemaAttemptSpec() -> int {
    client: "openai/gpt-4o-mini"
    prompt: `Return one integer. ${ctx.output_format()}`
}

class InvocationProbe {
    count: int,
}

class InvalidSchemaClient {
    probe: InvocationProbe,

    implements ai.Client {
        function requested_model(self) -> string {
            "invalid-schema-client"
        }
        function provider(self) -> string {
            "test"
        }

        function render(self, input: ai.ModelTurnInput) -> baml.http.Request {
            let _ = input;
            baml.http.Request {
                method: "POST",
                url: "https://example.invalid/schema-attempts",
                headers: {},
                body: "{}",
            }
        }

        function invoke(self, input: ai.ModelTurnInput) -> ai.ModelTurn {
            let _ = input;
            self.probe.count += 1;
            ai.ModelTurn { calls: [],
                child_turns: [],
                content: [ai.content.Text { text: "schema-mismatch" }],
                stop_reason: ai.content.StopReason.Complete,
            }
        }
    }
}

function run_invalid_schema(agent: ai.Agent) -> string {
    let _ = agent.run(SchemaAttemptSpec@spec());
    "accepted"
}

function schema_attempt_validation_message(schema_attempts: int) -> string {
    let _ = ai.Agent.new(schema_attempts = schema_attempts);
    "accepted"
}

test "default Agent makes one total schema attempt" {
    let probe = InvocationProbe { count: 0 };
    let scripted = InvalidSchemaClient { probe: probe };
    let agent = ai.Agent.new(client = scripted);

    let raw_output = run_invalid_schema(agent) catch (e) {
        ai.errors.ParseFailed => e.raw_output,
    };

    assert.equal(raw_output, "schema-mismatch");
    assert.equal(probe.count, 1)
}

test "configured Agent makes one total schema attempt" {
    let probe = InvocationProbe { count: 0 };
    let scripted = InvalidSchemaClient { probe: probe };
    let agent = ai.Agent.new(client = scripted, schema_attempts = 1);

    let raw_output = run_invalid_schema(agent) catch (e) {
        ai.errors.ParseFailed => e.raw_output,
    };

    assert.equal(raw_output, "schema-mismatch");
    assert.equal(probe.count, 1)
}

test "configured Agent makes two total schema attempts" {
    let probe = InvocationProbe { count: 0 };
    let scripted = InvalidSchemaClient { probe: probe };
    let agent = ai.Agent.new(client = scripted, schema_attempts = 2);

    let raw_output = run_invalid_schema(agent) catch (e) {
        ai.errors.ParseFailed => e.raw_output,
    };

    assert.equal(raw_output, "schema-mismatch");
    assert.equal(probe.count, 2)
}

test "configured Agent makes three total schema attempts" {
    let probe = InvocationProbe { count: 0 };
    let scripted = InvalidSchemaClient { probe: probe };
    let agent = ai.Agent.new(client = scripted, schema_attempts = 3);

    let raw_output = run_invalid_schema(agent) catch (e) {
        ai.errors.ParseFailed => e.raw_output,
    };

    assert.equal(raw_output, "schema-mismatch");
    assert.equal(probe.count, 3)
}

test "Agent rejects a non-positive schema attempt count" {
    let zero_message = schema_attempt_validation_message(0) catch (e) {
        baml.errors.InvalidArgument => e.message,
    };
    let negative_message = schema_attempt_validation_message(-1) catch (e) {
        baml.errors.InvalidArgument => e.message,
    };

    assert.equal(zero_message, "Agent schema_attempts must be at least 1");
    assert.equal(negative_message, "Agent schema_attempts must be at least 1")
}

test "Agent rejects a mutated schema attempt count before invoking" {
    let probe = InvocationProbe { count: 0 };
    let event_probe = InvocationProbe { count: 0 };
    let scripted = InvalidSchemaClient { probe: probe };
    let on_event = (event: ai.events.Event) -> void {
        let _ = event;
        event_probe.count += 1;
    };
    let agent = ai.Agent.new(client = scripted, on_event = on_event);
    agent.schema_attempts = 0;

    let message = run_invalid_schema(agent) catch (e) {
        baml.errors.InvalidArgument => e.message,
    };

    assert.equal(message, "Agent schema_attempts must be at least 1");
    assert.equal(probe.count, 0);
    assert.equal(event_probe.count, 0)
}

// A plain call is the default Agent, so it never re-asks: one request, and the
// ParseFailed carries that first reply. The client scripts a different reply
// per invocation, so a second request would change `raw_output`.

class SequencedReplyClient {
    probe: InvocationProbe,
    replies: string[],

    implements ai.Client {
        function requested_model(self) -> string {
            "sequenced-reply-client"
        }
        function provider(self) -> string {
            "test"
        }

        function render(self, input: ai.ModelTurnInput) -> baml.http.Request {
            let _ = input;
            baml.http.Request {
                method: "POST",
                url: "https://example.invalid/schema-attempts",
                headers: {},
                body: "{}",
            }
        }

        function invoke(self, input: ai.ModelTurnInput) -> ai.ModelTurn {
            let _ = input;
            let reply = self.replies.at(self.probe.count) ?? "out-of-replies";
            self.probe.count += 1;
            ai.ModelTurn {
                calls: [],
                child_turns: [],
                content: [ai.content.Text { text: reply }],
                stop_reason: ai.content.StopReason.Complete,
            }
        }
    }
}

function sequenced_client(probe: InvocationProbe) -> SequencedReplyClient {
    SequencedReplyClient {
        probe: probe,
        replies: ["first-mismatch", "42"],
    }
}

function direct_call_raw_output(client: ai.Client) -> string {
    let value = SchemaAttemptSpec(client = client);
    `accepted ${value}`
}

function spec_call_raw_output(client: ai.Client) -> string {
    let value = SchemaAttemptSpec@spec().call(client = client);
    `accepted ${value}`
}

test "direct LLM function call makes one request and throws the first reply" {
    let probe = InvocationProbe { count: 0 };
    let raw_output = direct_call_raw_output(sequenced_client(probe)) catch (e) {
        ai.errors.ParseFailed => e.raw_output,
    };

    assert.equal(raw_output, "first-mismatch");
    assert.equal(probe.count, 1)
}

test "spec call makes one request and throws the first reply" {
    let probe = InvocationProbe { count: 0 };
    let raw_output = spec_call_raw_output(sequenced_client(probe)) catch (e) {
        ai.errors.ParseFailed => e.raw_output,
    };

    assert.equal(raw_output, "first-mismatch");
    assert.equal(probe.count, 1)
}

test "opt-in re-ask on a spec recovers with the second reply" {
    let probe = InvocationProbe { count: 0 };
    let agent = ai.Agent.new(client = sequenced_client(probe), schema_attempts = 2);

    assert.equal(agent.run(SchemaAttemptSpec@spec()).value, 42);
    assert.equal(probe.count, 2)
}
