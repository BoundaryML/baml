client TestClient = openai.ResponsesClient.new(
    model = "gpt-4o-mini",
    api_key = "test-key",
    base_url = "http://localhost:1234",
);

function Classify<T>(input: string) -> T {
    client: TestClient
    prompt: `Choose a category for ${input}.\n${ctx.output_format()}`
}

function main() -> string throws never {
    let t = reflect.enum.new("Category", []) catch (e) {
        _ => {
            return "constructor threw";
        }
    };
    type T = unreflect(t)
    let rendered = Classify@render_prompt<T>("sample") catch (e) {
        reflect.errors.CompilationError => e.diagnostics[0].code + "|" + e.diagnostics[0].message,
        _ => "wrong render error",
    };
    if (rendered is string) {
        return rendered;
    }
    return "render did not throw";
}

test "empty_runtime_enum_fails_at_the_render_boundary" {
    let result = main();
    assert.is_true(result.starts_with("E0159|"));
    assert.contains(result, "empty enum `Category` cannot be rendered")
}
