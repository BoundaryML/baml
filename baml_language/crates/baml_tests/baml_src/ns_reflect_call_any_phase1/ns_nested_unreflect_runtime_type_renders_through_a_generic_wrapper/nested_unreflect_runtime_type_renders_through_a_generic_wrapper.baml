client TestClient = openai.ResponsesClient.new(
    model = "gpt-4o-mini",
    api_key = "test-key",
    base_url = "http://localhost:1234",
)

class Wrapper<T> {
    value: T,
}

function Extract<T>(input: string) -> T {
    client: TestClient
    prompt: `Extract ${input}.\n${ctx.output_format()}`
}

function main() -> string {
    let runtime_class = reflect.Class.new(
        "RuntimeTranscript",
        { "speaker": string.type(), "words": reflect.Type.of<string[]>() },
    );
    let type Transcript = runtime_class.as_type();
    Extract@render_prompt<Wrapper<Transcript>>("sample").text()
}

test "nested_unreflect_runtime_type_renders_through_a_generic_wrapper" {
    let prompt = main();
    assert.contains(prompt, "value:");
    assert.contains(prompt, "speaker");
    assert.contains(prompt, "words")
}
