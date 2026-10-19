// Offline and unchanged between nightly and the local build.
class Answer {
    value: int,
}

function Extract(input: string) -> Answer {
    client: "openai/gpt-4o-mini"
    prompt: `Extract ${input}. ${ctx.output_format()}`
}

class Scripted {
    chunks: string[],

    implements ai.Client {
        function id(self) -> string {
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
                content: [],
                calls: [],
                stop_reason: ai.content.StopReason.Complete,
                usage: null,
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

function main() -> string {
    consume("success", ["{\"value\":", "42}"]);
    consume("parse failure", ["not valid output"]);
    abandon();
    "done"
}
