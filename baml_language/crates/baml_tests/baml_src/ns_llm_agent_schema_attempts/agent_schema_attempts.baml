// Offline LLM regressions for the schema-correction budget on the real ai.Agent.
// The scripted client always returns a complete turn that cannot parse as int.

function SchemaAttemptSpec() -> int {
    client: "openai/gpt-4o-mini"
    prompt: `Return one integer. ${ctx.output_format}`
}

class InvocationProbe {
    count: int,
}

class InvalidSchemaClient {
    probe: InvocationProbe,

    implements ai.Client {
        function id(self) -> string {
            "invalid-schema-client"
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
                content: [ai.content.Text { text: "schema-mismatch" }],
                stop_reason: ai.content.StopReason.Complete,
                usage: null,
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

test "default Agent makes two total schema attempts" {
    let probe = InvocationProbe { count: 0 };
    let scripted = InvalidSchemaClient { probe: probe };
    let agent = ai.Agent.new(client = scripted);

    let raw_output = run_invalid_schema(agent) catch (e) {
        ai.errors.ParseFailed => e.raw_output,
    };

    assert.equal(raw_output, "schema-mismatch");
    assert.equal(probe.count, 2)
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
