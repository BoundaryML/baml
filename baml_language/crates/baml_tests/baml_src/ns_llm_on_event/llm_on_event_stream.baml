// Offline regressions for the unified `on_event` listener on the `$stream`
// path. A scripted streaming client replays a fixed event sequence, so the
// settle events (AssistantMessage, then Usage) and the recovered TurnMeta
// token counts are deterministic on both drain paths.

class StreamScript {
    events: (ai.stream.StreamEvent | ai.stream.Done)[],
    cursor: int,

    implements ai.Client {
        function id(self) -> string {
            "scripted-stream"
        }

        function render(self, input: ai.ModelTurnInput) -> baml.http.Request {
            let _ = input;
            baml.http.Request { method: "POST", url: "https://example.invalid/x", headers: {}, body: "{}" }
        }

        function invoke(self, input: ai.ModelTurnInput) -> ai.ModelTurn {
            let _ = input;
            ai.ModelTurn { content: [], stop_reason: ai.content.StopReason.Complete, usage: null }
        }
    }

    implements ai.stream.StreamingClient {
        function invoke_stream(self, input: ai.ModelTurnInput) -> ai.stream.TurnStream {
            let _ = input;
            ai.stream.TurnStream.from_event_source(() -> {
                let e = self.events.at(self.cursor);
                self.cursor += 1;
                match (e) {
                    let ev: ai.stream.StreamEvent => ev,
                    let d: ai.stream.Done => d,
                    null => ai.stream.Done {  },
                }
            })
        }
    }
}

function make_script() -> StreamScript {
    StreamScript {
        events: [
            ai.stream.TextDelta { text: "{\"message\":" },
            ai.stream.TextDelta { text: " \"hi\"}" },
            ai.stream.TurnMeta { stop_reason: ai.content.StopReason.Complete, input_tokens: 7, output_tokens: 11 },
            ai.stream.TurnDone {  },
        ],
        cursor: 0,
    }
}

test "stream drained via next delivers settle events exactly once" {
    let probe = Probe { labels: [] };
    let cb = (e: ai.events.Event) -> void {
        probe.labels.push(label_of(e));
    };
    let s = Greet$stream("world", client = make_script(), on_event = cb);
    while (true) {
        match (s.next()) {
            let d: ai.stream.Done => {
                break;
            },
            _ => {},
        };
    }
    // extra pulls after Done must not re-emit
    let _ = s.next();
    let f = s.final();
    assert.equal(f.message, "hi");
    assert.equal(probe.labels, ["RunStarted", "AssistantMessage", "Usage"])
}

test "stream drained via final recovers streamed usage" {
    let probe = Probe { labels: [] };
    let cb = (e: ai.events.Event) -> void {
        match (e) {
            let u: ai.events.Usage => {
                probe.labels.push(`Usage:${u.input_tokens}/${u.output_tokens}`);
            },
            _ => {
                probe.labels.push(label_of(e));
            },
        };
    };
    let s = Greet$stream("world", client = make_script(), on_event = cb);
    let f = s.final();
    assert.equal(f.message, "hi");
    assert.equal(probe.labels, ["RunStarted", "AssistantMessage", "Usage:7/11"])
}

test "throwing listener does not alter the stream result" {
    let cb = (e: ai.events.Event) -> void {
        throw baml.errors.InvalidArgument { message: "listener boom" }
    };
    let s = Greet$stream("world", client = make_script(), on_event = cb);
    let f = s.final();
    assert.equal(f.message, "hi")
}
