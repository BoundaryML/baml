// Live streaming smokes. Not tests — run with:
//   infisical run --env=test -- baml-cli run -e 'live_openai_stream()'
//   infisical run --env=test -- baml-cli run -e 'live_anthropic_stream()'
//   infisical run --env=test -- baml-cli run -e 'live_google_stream()'

function StreamAnswer(question: string) -> string {
    client "openai/gpt-4o-mini"
    tools []
    prompt `
        Answer in two or three sentences.
        ${question}
        ${ctx.output_format}
    `
}

// Drain the partial stream, then take the final value; asserts the stream
// really streamed (more than one next() yield) and produced a final value.
// The stream-expanded partial type of `string` is `string?`.
function _drain(s: baml.llm.Stream<string?, string>) -> string {
    let yields = 0;
    let last_len = 0;
    while (true) {
        match (s.next()) {
            let done: baml.stream.StreamFinished => break,
            let p: string => {
                yields += 1;
                last_len = p.length();
                null
            },
            null => {
                yields += 1;
                null
            },
        };
    }
    let value = s.final();
    assert.is_true(yields > 1);
    assert.is_true(value.length() > 0);
    `yields=${yields} last_partial_len=${last_len} final_len=${value.length()}`
}

function live_openai_stream() -> string {
    // No client override: exercises the spec's default client through the
    // runtime StreamingClient downcast.
    _drain(StreamAnswer$stream(question = "Why is the sky blue?"))
}

function live_anthropic_stream() -> string {
    _drain(
        StreamAnswer$stream(
            question = "Why is the sky blue?",
            client = anthropic.AnthropicClient.new(),
        ),
    )
}

function live_google_stream() -> string {
    _drain(
        StreamAnswer$stream(
            question = "Why is the sky blue?",
            client = google.GoogleClient.new(),
        ),
    )
}
