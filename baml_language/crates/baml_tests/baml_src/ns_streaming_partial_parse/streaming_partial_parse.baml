// Offline tests for partial parsing in `ai.stream.Stream`. A partial that can't
// be parsed yet (an opening ```json fence, or prose before the JSON) is skipped,
// as in BAML v0; only the final parse reports a malformed reply.

class Note {
    title: string,
    body: string,
}

function ExtractNote(input: string) -> Note {
    client: "openai/gpt-4o-mini"
    prompt: `Summarize ${input}. ${ctx.output_format()}`
}

class ChunkedStreamClient {
    chunks: string[],

    implements ai.Client {
        function id(self) -> string {
            "chunked"
        }

        function render(self, input: ai.ModelTurnInput) -> baml.http.Request {
            let _ = input;
            baml.http.Request {
                method: "POST",
                url: "https://example.invalid/chunked",
                headers: {},
                body: "{}",
            }
        }

        function invoke(self, input: ai.ModelTurnInput) -> ai.ModelTurn {
            let _ = input;
            ai.ModelTurn {
                calls: [],
                content: [ai.content.Text { text: self.chunks.join("") }],
                stop_reason: ai.content.StopReason.Complete,
                usage: null,
            }
        }
    }

    implements ai.stream.StreamingClient {
        function invoke_stream(self, input: ai.ModelTurnInput) -> ai.stream.TurnStream {
            let _ = input;
            ai.stream.TurnStream.from_chunks(self.chunks)
        }
    }
}

// Drains `chunks` through `ExtractNote@stream`: "<any partials>|<final title>/<final body>".
function stream_note(chunks: string[]) -> string {
    let stream = ai.stream.from_spec<Note>(
        ExtractNote@spec("x"),
        client = ChunkedStreamClient { chunks: chunks },
    );
    let titles: string[] = [];
    while let partial: Note = stream.next() {
        titles.push(partial.title);
    }
    let note = stream.final();
    `${titles.length() > 0}|${note.title}/${note.body}`
}

test "stream_skips_partials_before_a_json_fence_opens" {
    assert.equal(
        stream_note(["```", "json\n", "{\"title\": \"Hi\", ", "\"body\": \"there\"}", "\n```"]),
        "true|Hi/there",
    )
}

test "stream_skips_partials_for_prose_before_the_json" {
    assert.equal(
        stream_note(["Here", " is the JSON:\n", "{\"title\": \"Hi\", ", "\"body\": \"there\"}"]),
        "true|Hi/there",
    )
}

function stream_unparseable_reply() -> string {
    let stream = ai.stream.from_spec<Note>(
        ExtractNote@spec("x"),
        client = ChunkedStreamClient { chunks: ["no", " json", " here"] },
    );
    let partials = 0;
    while let partial: Note = stream.next() {
        let _ = partial;
        partials += 1;
    }
    let final_outcome = stream.final().title catch_all (e) {
        _ => "final-threw",
    };
    `${partials}|${final_outcome}`
}

test "stream_still_reports_an_unparseable_final_reply" {
    assert.equal(stream_unparseable_reply(), "0|final-threw")
}
