// Offline regressions for the unified `on_event` listener on the direct-call
// path and the explicit Agent. A scripted client returns a canned parseable
// reply so event order and counts are deterministic.

class Greeting {
    message: string,
}

function Greet(name: string) -> Greeting {
    client: "openai/gpt-4o-mini"
    prompt: `
        ${role("user")}
        Greet ${name}.

        ${ctx.output_format()}
    `
}

class Probe {
    labels: string[],
}

class ScriptedClient {
    implements ai.Client {
        function id(self) -> string {
            "scripted"
        }

        function render(self, input: ai.ModelTurnInput) -> baml.http.Request {
            let _ = input;
            baml.http.Request { method: "POST", url: "https://example.invalid/x", headers: {}, body: "{}" }
        }

        function invoke(self, input: ai.ModelTurnInput) -> ai.ModelTurn {
            let _ = input;
            ai.ModelTurn { calls: [],
                content: [ai.content.Text { text: "{\"message\": \"hi\"}" }],
                stop_reason: ai.content.StopReason.Complete,
                usage: ai.events.Usage { input_tokens: 3, output_tokens: 5, cached_input_tokens: null, reasoning_tokens: null },
            }
        }
    }
}

function label_of(e: ai.events.Event) -> string {
    match (e) {
        let x: ai.events.RunStarted => "RunStarted",
        let x: ai.events.UserMessage => "UserMessage",
        let x: ai.events.UserContent => "UserContent",
        let x: ai.events.AssistantMessage => "AssistantMessage",
        let x: ai.events.ToolRequested => "ToolRequested",
        let x: ai.events.ToolCompleted => "ToolCompleted",
        let x: ai.events.ToolFailed => "ToolFailed",
        let x: ai.events.Usage => "Usage",
        let x: ai.events.LLMCall => "LLMCall",
        let x: ai.events.FinalProduced => "FinalProduced",
    }
}

test "direct call delivers events in journal order" {
    let probe = Probe { labels: [] };
    let cb = (e: ai.events.Event) -> void {
        probe.labels.push(label_of(e));
    };
    let g = Greet("world", client = ScriptedClient {  }, on_event = cb);
    assert.equal(g.message, "hi");
    assert.equal(probe.labels, ["RunStarted", "AssistantMessage", "Usage", "FinalProduced"])
}

test "throwing listener does not alter the direct-call result" {
    let cb = (e: ai.events.Event) -> void {
        throw baml.errors.InvalidArgument { message: "listener boom" }
    };
    let g = Greet("world", client = ScriptedClient {  }, on_event = cb);
    assert.equal(g.message, "hi")
}

test "explicit Agent accepts and swallows a throwing listener" {
    let probe = Probe { labels: [] };
    let cb = (e: ai.events.Event) -> void {
        probe.labels.push(label_of(e));
        throw baml.errors.InvalidArgument { message: "listener boom" }
    };
    let agent = ai.Agent.new(client = ScriptedClient {  }, on_event = cb);
    let out = agent.run(Greet@spec("world"));
    assert.equal(out.value.message, "hi");
    assert.equal(probe.labels, ["RunStarted", "AssistantMessage", "Usage", "FinalProduced"])
}

// The Claude Code CLI folds the journal into one text prompt: a UserContent
// turn renders its text parts as written and its media as a readable
// placeholder, in order, after the tool result it follows.
function claude_code_transcript_with_user_content() -> string {
    let args: map<string, unknown> = { };
    let journal = ai.Journal {
        log: [
            ai.events.AssistantMessage {
                content: [ai.content.ToolUse { id: "c1", name: "look", args: args }],
                client_id: "claude-code/opus",
            },
            ai.events.ToolCompleted { id: "c1", output: "done" },
            ai.events.UserContent.new(
                [
                    "Screenshot after the tool ran: ",
                    baml.media.Image.from_base64("SGVsbG8=", "image/png"),
                ],
            ),
        ],
    };
    claude_code.internal.transcript_text(journal)
}

test "claude_code_transcript_with_user_content" {
    let got = claude_code_transcript_with_user_content();
    assert.contains(got, "tool result id=c1 is_error=false: done\nuser: Screenshot after the tool ran: [image]")
}
