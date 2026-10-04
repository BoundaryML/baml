// Offline regressions for which Text block of a turn ai.Agent parses.
//
// A provider can split one answer over several messages: OpenAI Responses
// models send `commentary` messages before the `final_answer` message, and
// the commentary sometimes holds the answer while the final message is empty
// or holds no answer. The runner parses the last non-blank Text block that
// fits the output type, then the ones before it.
//
// Per the corpus convention every case body is a top-level `function` and the
// `test` block only asserts.

class PickedTool {
    tool: string,
}

function PickToolSpec() -> PickedTool {
    client: "openai/gpt-4o-mini"
    prompt: `Pick a tool. ${ctx.output_format()}`
}

function SayWordSpec() -> string {
    client: "openai/gpt-4o-mini"
    prompt: `Say one word.`
}

class TextBlocksProbe {
    invocations: int,
}

/// Answers every invocation with one Complete turn that has a Text block for
/// each entry of `texts`, in order.
class TextBlocksClient {
    texts: string[],
    probe: TextBlocksProbe,

    implements ai.Client {
        function id(self) -> string {
            "text-blocks-client"
        }

        function render(self, input: ai.ModelTurnInput) -> baml.http.Request {
            let _ = input;
            baml.http.Request {
                method: "POST",
                url: "https://example.invalid/text-blocks",
                headers: {},
                body: "{}",
            }
        }

        function invoke(self, input: ai.ModelTurnInput) -> ai.ModelTurn {
            let _ = input;
            self.probe.invocations += 1;
            let content: ai.content.ModelBlock[] = [];
            for (let text in self.texts) {
                content.push(ai.content.Text { text: text });
            }
            ai.ModelTurn {
                calls: [],
                content: content,
                stop_reason: ai.content.StopReason.Complete,
                usage: null,
            }
        }
    }
}

/// The parsed tool and the number of model invocations, as `tool|invocations`.
function picked_tool(texts: string[]) -> string {
    let probe = TextBlocksProbe { invocations: 0 };
    let agent = ai.Agent.new(client = TextBlocksClient { texts: texts, probe: probe });
    let tool = agent.run(PickToolSpec@spec()).value.tool;
    `${tool}|${probe.invocations}`
}

/// The ParseFailed raw output and the number of model invocations, with one
/// schema attempt.
function picked_tool_failure(texts: string[]) -> string {
    let probe = TextBlocksProbe { invocations: 0 };
    let agent = ai.Agent.new(client = TextBlocksClient { texts: texts, probe: probe }, schema_attempts = 1);
    let outcome = picked_tool_value(agent) catch (e) {
        ai.errors.ParseFailed => `ParseFailed:${e.raw_output}`,
    };
    `${outcome}|${probe.invocations}`
}

function picked_tool_value(agent: ai.Agent) -> string {
    agent.run(PickToolSpec@spec()).value.tool
}

function said_word(texts: string[]) -> string {
    let probe = TextBlocksProbe { invocations: 0 };
    let agent = ai.Agent.new(client = TextBlocksClient { texts: texts, probe: probe });
    agent.run(SayWordSpec@spec()).value
}

test "the final message wins when it parses" {
    assert.equal(
        picked_tool([`{"tool": "from_commentary"}`, `{"tool": "from_final"}`]),
        "from_final|1",
    )
}

test "a blank final message falls back to the commentary" {
    assert.equal(picked_tool([`{"tool": "from_commentary"}`, ""]), "from_commentary|1");
    assert.equal(picked_tool([`{"tool": "from_commentary"}`, " \n"]), "from_commentary|1")
}

test "a final message that does not parse falls back to the commentary" {
    assert.equal(
        picked_tool([`{"tool": "from_commentary"}`, "I could not decide."]),
        "from_commentary|1",
    )
}

test "earlier messages are tried from the last to the first" {
    assert.equal(picked_tool([`{"tool": "first"}`, `{"tool": "second"}`, ""]), "second|1")
}

test "when no message parses ParseFailed carries the whole text output" {
    assert.equal(
        picked_tool_failure(["Let me think. ", "Still thinking."]),
        "ParseFailed:Let me think. Still thinking.|1",
    )
}

test "a string output skips a blank final message" {
    assert.equal(said_word(["hello", ""]), "hello");
    assert.equal(said_word(["draft", "final"]), "final")
}

test "a turn without text parses the empty string as before" {
    assert.equal(said_word([]), "")
}
