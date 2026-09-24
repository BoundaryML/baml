client TestClient = openai.ResponsesClient.new(
            model = "gpt-4o-mini",
            api_key = "test-key",
            base_url = "http://localhost:1234",
        );

type PromptFn = (topic: string) -> ai.Prompt throws unknown;

function Plain(topic: string) -> string {
    client: TestClient
    prompt: `
        Say ${topic}.
        ${ctx.output_format()}
    `
}

function main() -> bool {
    let package = reflect.Package.current();
    let callable = package.get_function<PromptFn>(
        "root.reflect_call_any_phase1.get_function_still_extracts_a_non_generic_companion.Plain@render_prompt",
    )
        ?? throw "expected the companion";
    let rendered = callable("hello");
    rendered.text().length() > 0
}

test "get_function_still_extracts_a_non_generic_companion" {
    assert.equal(main(), true)
}
