// Offline regressions for which Text block of a turn ai.Agent parses.
//
// A provider can split one answer over several messages. OpenAI Responses
// models label them: `commentary` messages that hold the model's next step
// (its tool calls), then a `final_answer` written as if that step had run. For
// a structured output the runner tries the commentary messages first, in
// order, then the other blocks from the last to the first. Unlabeled blocks
// and text-shaped outputs use the last-to-first order only.
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

/// Answers every invocation with one Complete turn whose content is `blocks`.
class TextBlocksClient {
    blocks: ai.content.Text[],
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
            for (let block in self.blocks) {
                content.push(block);
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

/// Text blocks without a phase, as most providers send them.
function unlabeled(texts: string[]) -> ai.content.Text[] {
    let blocks: ai.content.Text[] = [];
    for (let text in texts) {
        blocks.push(ai.content.Text { text: text });
    }
    blocks
}

function commentary(text: string) -> ai.content.Text {
    ai.content.Text { text: text, phase: "commentary" }
}

function final_answer(text: string) -> ai.content.Text {
    ai.content.Text { text: text, phase: "final_answer" }
}

/// The parsed tool and the number of model invocations, as `tool|invocations`.
function picked_tool(blocks: ai.content.Text[]) -> string {
    let probe = TextBlocksProbe { invocations: 0 };
    let agent = ai.Agent.new(client = TextBlocksClient { blocks: blocks, probe: probe });
    let tool = agent.run(PickToolSpec@spec()).value.tool;
    `${tool}|${probe.invocations}`
}

/// The ParseFailed raw output and the number of model invocations, with one
/// schema attempt.
function picked_tool_failure(blocks: ai.content.Text[]) -> string {
    let probe = TextBlocksProbe { invocations: 0 };
    let agent = ai.Agent.new(client = TextBlocksClient { blocks: blocks, probe: probe }, schema_attempts = 1);
    let outcome = picked_tool_value(agent) catch (e) {
        ai.errors.ParseFailed => `ParseFailed:${e.raw_output}`,
    };
    `${outcome}|${probe.invocations}`
}

function picked_tool_value(agent: ai.Agent) -> string {
    agent.run(PickToolSpec@spec()).value.tool
}

function said_word(blocks: ai.content.Text[]) -> string {
    let probe = TextBlocksProbe { invocations: 0 };
    let agent = ai.Agent.new(client = TextBlocksClient { blocks: blocks, probe: probe });
    agent.run(SayWordSpec@spec()).value
}

test "a commentary message wins over the final answer" {
    assert.equal(
        picked_tool([commentary(`{"tool": "from_commentary"}`), final_answer(`{"tool": "from_final"}`)]),
        "from_commentary|1",
    )
}

test "commentary messages are tried in order before the final answer" {
    assert.equal(
        picked_tool(
            [
                commentary(`{"tool": "first"}`),
                commentary(`{"tool": "second"}`),
                final_answer(`{"tool": "from_final"}`),
            ],
        ),
        "first|1",
    );
    assert.equal(
        picked_tool(
            [
                commentary("Let me check your gyms."),
                commentary(`{"tool": "second"}`),
                final_answer(`{"tool": "from_final"}`),
            ],
        ),
        "second|1",
    )
}

test "a commentary message that does not parse falls back to the final answer" {
    assert.equal(
        picked_tool([commentary("Let me check your gyms."), final_answer(`{"tool": "from_final"}`)]),
        "from_final|1",
    )
}

test "a blank final answer leaves the commentary" {
    assert.equal(
        picked_tool([commentary(`{"tool": "from_commentary"}`), final_answer("")]),
        "from_commentary|1",
    )
}

test "an unlabeled final message wins when it parses" {
    assert.equal(
        picked_tool(unlabeled([`{"tool": "earlier"}`, `{"tool": "from_final"}`])),
        "from_final|1",
    )
}

test "a blank or unparseable unlabeled final message falls back to the one before it" {
    assert.equal(picked_tool(unlabeled([`{"tool": "earlier"}`, ""])), "earlier|1");
    assert.equal(picked_tool(unlabeled([`{"tool": "earlier"}`, " \n"])), "earlier|1");
    assert.equal(
        picked_tool(unlabeled([`{"tool": "earlier"}`, "I could not decide."])),
        "earlier|1",
    )
}

test "unlabeled messages are tried from the last to the first" {
    assert.equal(
        picked_tool(unlabeled([`{"tool": "first"}`, `{"tool": "second"}`, ""])),
        "second|1",
    )
}

test "when no message parses ParseFailed carries the whole text output" {
    assert.equal(
        picked_tool_failure([commentary("Let me think. "), final_answer("Still thinking.")]),
        "ParseFailed:Let me think. Still thinking.|1",
    )
}

test "a string output prefers the final answer over a commentary preamble" {
    assert.equal(said_word([commentary("Let me check."), final_answer("hello")]), "hello");
    assert.equal(said_word([commentary("hello"), final_answer("")]), "hello")
}

test "a string output skips a blank unlabeled final message" {
    assert.equal(said_word(unlabeled(["hello", ""])), "hello");
    assert.equal(said_word(unlabeled(["draft", "final"])), "final")
}

test "a turn without text parses the empty string as before" {
    assert.equal(said_word([]), "")
}
