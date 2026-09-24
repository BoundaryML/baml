client TestClient = openai.ResponsesClient.new(
    model = "gpt-4o-mini",
    api_key = "test-key",
    base_url = "http://localhost:1234",
);

function Classify<T>(input: string) -> T {
    client: TestClient
    prompt: `Choose a category for ${input}.\n${ctx.output_format()}`
}

function main() -> string {
    // Widen explicitly (`as_type`) to select `reflect.Type.meta(...)`,
    // rather than the enum-kind view's zero-argument metadata reader.
    let left: reflect.Type = reflect.enum.new("Category", ["RED", "BLUE"]).as_type();
    let right: reflect.Type = reflect.enum.new("Category", ["RED", "BLUE"]).as_type();
    type Left = unreflect(left)
    type Right = unreflect(right)
    let left_prompt = Classify@render_prompt<Left>("sample").text();
    let right_prompt = Classify@render_prompt<Right>("sample").text();
    let tagged = left.meta(
        alias = "category_code",
        description = "A generated category",
        docstring = "runtime docs",
        other = { "owner": "tests" },
    );
    let owner = tagged.other.get("owner");
    return (left != right).to_string()
        + "|"
        + (left_prompt == right_prompt).to_string()
        + "|"
        + (tagged.ty == left).to_string()
        + "|"
        + (tagged.alias ?? "null")
        + "|"
        + (tagged.description ?? "null")
        + "|"
        + (tagged.docstring ?? "null")
        + "|"
        + (owner ?? "null");
}

test "runtime_enum_identity_and_metadata_are_preserved" {
    assert.equal(main(), "true|true|true|category_code|A generated category|runtime docs|tests")
}
