// Worked example: driving a real provider client against the mock servers in
// mock_provider.baml. This is the template every per-provider test namespace
// copies — non-streaming through `mock_json_serve`, streaming through
// `mock_sse_serve`.
//
// The ephemeral mock port is only known at runtime, and BAML has no `env.set`,
// so a `client Foo = …` *declaration* (evaluated during `$init`) can never point
// at it. Construct the client inside the test function instead and hand it to
// the runner explicitly:
//
//   ai.Agent<Out>.new(client = cl).run(Fn@spec(args))        // non-streaming
//   ai.stream.from_spec<Out$stream, Out>(Fn@spec(args), client = cl)  // streaming
//
// which is exactly what a direct `Fn(args)` call desugars to
// (crates/baml_compiler2_ast/src/lower_expr_body.rs:729, :819). `from_spec`
// cannot infer its two type parameters from an argument, so they are written
// out.

function MockEcho(question: string) -> string {
    client: "openai/gpt-4o-mini"
    prompt: `
        Reply with exactly one word.
        ${question}
    `
}

// Non-streaming: the canned Responses body comes back as the run value, and the
// request the client put on the wire is available for assertions.
function example_openai_non_streaming() -> string {
    let canned = `{"status":"completed","output":[{"type":"message","content":[{"type":"output_text","text":"pong"}]}],"usage":{"input_tokens":3,"output_tokens":1}}`;
    root.llm_mock.mock_json_serve(
        [root.llm_mock.MockResponse.ok(canned)],
        (mock: MockJsonServer) -> {
            let cl = openai.ResponsesClient.new(
                model = "gpt-4o-mini",
                api_key = "test-key",
                base_url = mock.base_url(),
            );
            let result = ai.Agent<string>.new(client = cl).run(MockEcho@spec("Say pong."));
            let sent = mock.last_request();
            [
                result.value,
                sent.method,
                sent.path(),
                baml.json.path<string>(sent.json(), ".model"),
                baml.json.path<string>(sent.json(), ".input[0].content[0].text"),
            ].join("|")
        },
    )
}

test "example_openai_non_streaming" {
    let got = example_openai_non_streaming();
    // value | method | path | model | first rendered input part
    assert.contains(got, "pong|POST|/responses|gpt-4o-mini|");
    assert.contains(got, "Say pong.")
}

// Streaming: the SSE mock flushes each event as its own chunk, so the client's
// decoder sees real partials before the terminal event.
function example_openai_streaming() -> string {
    let done = `{"type":"response.completed","response":{"status":"completed","output":[{"type":"message","content":[{"type":"output_text","text":"pong"}]}],"usage":{"input_tokens":3,"output_tokens":1}}}`;
    root.llm_mock.mock_sse_serve(
        [
            root.llm_mock.sse_data(`{"type":"response.output_text.delta","delta":"po"}`),
            root.llm_mock.sse_data(`{"type":"response.output_text.delta","delta":"ng"}`),
            root.llm_mock.sse_data(done),
        ],
        (mock: MockSseServer) -> {
            let cl = openai.ResponsesClient.new(
                model = "gpt-4o-mini",
                api_key = "test-key",
                base_url = mock.base_url(),
            );
            let stream = ai.stream.from_spec<string | null, string>(
                MockEcho@spec("Say pong."),
                client = cl,
            );
            stream.final()
        },
    )
}

test "example_openai_streaming" {
    assert.equal(example_openai_streaming(), "pong")
}
