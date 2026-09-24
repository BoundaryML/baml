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
    let runtime_class = reflect.class.new(
        "RuntimeTranscript",
        { "speaker": reflect.Type.of<string>(), "words": reflect.Type.of<string[]>() },
    );
    type Transcript = unreflect(runtime_class.as_type())
    Extract@render_prompt<Wrapper<Transcript>>("sample").text()
}

test "nested_unreflect_runtime_type_renders_through_a_generic_wrapper" {
    let prompt = main();
    assert.contains(prompt, "value:");
    assert.contains(prompt, "speaker");
    assert.contains(prompt, "words")
}
