// The `on_event` listener of an `ai.Agent` run that ends with an error.
//
// A run records every turn in its journal before it judges the turn: the reply
// that did not parse, the turn the model cut short, the refusal. The listener
// gets every event that is in the journal when the run ends, in journal order
// and one time each, whether the run returns or throws. There is no event for
// the failure itself: a failed run ends with the error, a good one with
// `FinalProduced`.
//
// The scripted client plays one step of its script for each turn: a text to
// answer with, a stop reason, a tool call, or a throw.
//
// Per the corpus convention every case body is a top-level `function` and the
// `test` block only asserts.

class AeeValue {
    value: int,
}

function AeeExtract(text: string) -> AeeValue {
    client: "openai/gpt-4o-mini"
    prompt: `
        ${role("user")}
        ${text}

        ${ctx.output_format()}
    `
}

class AeeScriptedClient {
    script: string[],
    turn: int,
    with_usage: bool,

    implements ai.Client {
        function id(self) -> string {
            "agent-error-events-client"
        }

        function render(self, input: ai.ModelTurnInput) -> baml.http.Request {
            let _ = input;
            baml.http.Request { method: "POST", url: "https://example.invalid/agent-error-events", headers: {}, body: "{}" }
        }

        function invoke(self, input: ai.ModelTurnInput) -> ai.ModelTurn {
            let _ = input;
            let step = self.script.at(self.turn) ?? "throw";
            self.turn += 1;
            if (step == "throw") {
                throw ai.errors.InvalidRequest {
                    provider: "agent-error-events-client",
                    status_code: 400,
                    detail: "scripted failure",
                    raw_body: null,
                }
            }
            let content: ai.content.ModelBlock[] = [];
            let stop_reason: ai.content.StopReason = ai.content.StopReason.Complete;
            if (step == "max_tokens") {
                content.push(ai.content.Text { text: "{\"value\": " });
                stop_reason = ai.content.StopReason.MaxTokens;
            } else if (step == "refused") {
                content.push(ai.content.Text { text: "I will not answer that." });
                stop_reason = ai.content.StopReason.Refused;
            } else if (step == "tool") {
                let args: map<string, unknown> = { "reason": "scripted" };
                content.push(ai.content.ToolUse { id: `call_${self.turn}`, name: "aee_failing_tool", args: args });
                stop_reason = ai.content.StopReason.ToolUse;
            } else {
                content.push(ai.content.Text { text: step });
            }
            let usage: ai.events.Usage? = if (self.with_usage) {
                ai.events.Usage { input_tokens: 3, output_tokens: 5, cached_input_tokens: null, reasoning_tokens: null }
            } else {
                null
            };
            ai.ModelTurn { calls: [], content: content, stop_reason: stop_reason, usage: usage }
        }
    }
}

function aee_label(event: ai.events.Event) -> string {
    match (event) {
        let x: ai.events.RunStarted => "RunStarted",
        let x: ai.events.UserMessage => "UserMessage",
        let x: ai.events.AssistantMessage => "AssistantMessage",
        let x: ai.events.ToolRequested => "ToolRequested",
        let x: ai.events.ToolCompleted => "ToolCompleted",
        let x: ai.events.ToolFailed => "ToolFailed",
        let x: ai.events.Usage => "Usage",
        let x: ai.events.LLMCall => "LLMCall",
        let x: ai.events.FinalProduced => "FinalProduced",
    }
}

function aee_failing_tool(reason: string) -> string {
    throw baml.errors.InvalidArgument { message: `the tool failed: ${reason}` }
}

class AeeProbe {
    labels: string[],
}

/// Runs the script and returns `<how the run ended>: <the labels the listener
/// got>`. A good run also checks the listener against the journal it returns.
function aee_run(
    script: string[],
    schema_attempts: int,
    max_steps: int,
    tool_errors: ai.tools.ErrorMode,
    with_usage: bool,
) -> string {
    let probe = AeeProbe { labels: [] };
    let listen = (event: ai.events.Event) -> void {
        probe.labels.push(aee_label(event));
    };
    let agent = ai.Agent.new(
        client = AeeScriptedClient { script: script, turn: 0, with_usage: with_usage },
        schema_attempts = schema_attempts,
        max_steps = max_steps,
        tool_errors = tool_errors,
        on_event = listen,
    );
    let spec = AeeExtract@spec("How many?");
    spec.toolbox = ai.tools.Toolbox.new(
        [ai.tools.tool(aee_failing_tool, name = "aee_failing_tool", description = "Always fails.")],
    );
    let ended = aee_end_of_run(agent, spec, probe);
    `${ended}: ${probe.labels.join(",")}`
}

function aee_end_of_run(agent: ai.Agent, spec: ai.FunctionSpec<AeeValue>, probe: AeeProbe) -> string {
    let result = agent.run(spec) catch_all (e) {
        let failed: ai.errors.ParseFailed => {
            return "ParseFailed";
        },
        let failed: ai.errors.FinishReasonError => {
            return "FinishReasonError";
        },
        let failed: ai.errors.Refused => {
            return "Refused";
        },
        let failed: ai.errors.InvalidRequest => {
            return "InvalidRequest";
        },
        let failed: ai.errors.ToolFailedError => {
            return "ToolFailedError";
        },
        let failed: ai.errors.StepBudgetExceeded => {
            return "StepBudgetExceeded";
        },
        _ => {
            return `other: ${e.to_string()}`;
        },
    };
    let journal = result.journal.entries().map((event) -> { aee_label(event) });
    if (journal.join(",") == probe.labels.join(",")) {
        `value ${result.value.value}`
    } else {
        `the listener and the journal differ: ${journal.join(",")}`
    }
}

function aee_text_run(script: string[], schema_attempts: int) -> string {
    aee_run(script, schema_attempts, 12, ai.tools.ErrorMode.Report, false)
}

test "a good run delivers the journal and ends with FinalProduced" {
    assert.equal(aee_text_run([`{"value": 1}`], 1), "value 1: RunStarted,AssistantMessage,FinalProduced")
}

test "a reply that does not parse is delivered before ParseFailed" {
    assert.equal(aee_text_run(["prose"], 1), "ParseFailed: RunStarted,AssistantMessage")
}

test "every failed schema attempt is delivered, with its correction request" {
    assert.equal(
        aee_text_run(["prose", "more prose", "still prose"], 3),
        "ParseFailed: RunStarted,AssistantMessage,UserMessage,AssistantMessage,UserMessage,AssistantMessage",
    );
    // A repaired run delivers the failed attempt once, and matches its journal.
    assert.equal(
        aee_text_run(["prose", `{"value": 2}`], 2),
        "value 2: RunStarted,AssistantMessage,UserMessage,AssistantMessage,FinalProduced",
    )
}

test "a cut-short turn and a refusal are delivered before their errors" {
    assert.equal(aee_text_run(["max_tokens"], 2), "FinishReasonError: RunStarted,AssistantMessage");
    assert.equal(aee_text_run(["refused"], 2), "Refused: RunStarted,AssistantMessage")
}

test "the usage of a failed turn is delivered with it" {
    assert.equal(
        aee_run(["prose"], 1, 12, ai.tools.ErrorMode.Report, true),
        "ParseFailed: RunStarted,AssistantMessage,Usage",
    )
}

test "a client that throws leaves the earlier attempts delivered" {
    // The first attempt is on the record when the second invocation throws.
    assert.equal(aee_text_run(["prose", "throw"], 2), "InvalidRequest: RunStarted,AssistantMessage,UserMessage");
    // Nothing but the start of the run is on the record when the first throws.
    assert.equal(aee_text_run(["throw"], 2), "InvalidRequest: RunStarted")
}

test "a tool failure that ends the run is delivered once" {
    assert.equal(
        aee_run(["tool"], 2, 12, ai.tools.ErrorMode.Raise, false),
        "ToolFailedError: RunStarted,AssistantMessage,ToolRequested,ToolFailed",
    )
}

test "a run that spends its steps has delivered every step" {
    assert.equal(
        aee_run(["tool", "tool"], 2, 2, ai.tools.ErrorMode.Report, false),
        "StepBudgetExceeded: RunStarted,AssistantMessage,ToolRequested,ToolFailed,AssistantMessage,ToolRequested,ToolFailed",
    )
}
