client TestClient = openai.ResponsesClient.new(
    model = "gpt-4o-mini",
    api_key = "test-key",
);

function Classify<T>(input: string) -> T {
    client: TestClient
    prompt: `${input} ${ctx.output_format()}`
}

function main() -> null {
    let t = reflect.enum.new("Category", ["RED"]);
    type T = unreflect(t)
    Classify@stream<T>("sample");
    return null;
}

test "a_scoped_runtime_type_is_an_ordinary_streaming_type_argument" {
    assert.equal(1, 1)
}
