// Offline exercise of every owned-operation entry path.
class Answer {
    value: int,
}

function entry_policy() -> trace.Options throws never {
    trace.span(inputs = true, output = true, error = true).context(metadata = {
        "hook": "hook", "hook_only": "yes",
    })
}

/// baml:$trace=entry_policy
function Extract(input: string) -> Answer {
    client: "openai/gpt-4o-mini"
    prompt: `Extract ${input}. ${ctx.output_format()}`
}

class Scripted {
    chunks: string[],

    implements ai.Client {
        function requested_model(self) -> string {
            "scripted"
        }
        function render(self, input: ai.ModelTurnInput) -> baml.http.Request {
            baml.http.Request {
                method: "POST",
                url: "https://example.invalid/unused",
                headers: {},
                body: "",
            }
        }
        function invoke(self, input: ai.ModelTurnInput) -> ai.ModelTurn {
            ai.ModelTurn {
                content: [ai.content.Text { text: self.chunks.join("") }],
                calls: [],
                child_turns: [],
                stop_reason: ai.content.StopReason.Complete,
            }
        }
    }
    implements ai.stream.StreamingClient {
        function invoke_stream(self, input: ai.ModelTurnInput) -> ai.stream.TurnStream {
            ai.stream.TurnStream.from_chunks(self.chunks)
        }
    }
}

function consume(input: string, chunks: string[]) -> void {
    let stream = Extract@stream(input, client = Scripted { chunks: chunks });
    while (true) {
        match (stream.next()) {
            ai.stream.Done => {
                break;
            },
            _ => {},
        }
    }
    // No final() call: EOF must settle the logical operation itself.
}

function abandon() -> void {
    let stream = Extract@stream("abandoned", client = Scripted { chunks: ["{\"value\":", "42}"] });
    let _ = stream.next();
    // Drop without EOF. This is neither success nor evidence of cancellation.
}

function main(n: int) -> int {
    let provider = Scripted { chunks: ["{\"value\":42}"] };
    let reserved = trace.span(inputs = true, output = true, error = true)
        .context(metadata = { "hook": "reserved", "caller": "held" }).reserve();
    Extract("shared", client = provider, $trace = reserved);
    assert.equal(reserved.inspect().context.metadata.has("hook_only"), false);
    let f = Extract;
    f("shared", client = provider);
    let spec = Extract@spec("shared");
    ai.Agent.new(client = provider).run(spec);
    spec.call(client = provider);
    consume("shared", ["{\"value\":42}"]);
    consume("parse failure", ["invalid"]);
    abandon();
    42
}
