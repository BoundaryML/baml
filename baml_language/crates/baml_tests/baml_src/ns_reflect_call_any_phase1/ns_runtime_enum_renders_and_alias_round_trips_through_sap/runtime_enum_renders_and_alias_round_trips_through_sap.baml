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
    let t = reflect.enum.new(
        "Category",
        [
            reflect.enum.value("RED", alias = "k7", description = "warm"),
            reflect.enum.value("BLUE", description = "cool"),
        ],
    );
    type T = unreflect(t)
    let prompt = Classify@render_prompt<T>("sample").text();
    let parsed = Classify@parse<T>(`"k7"`);
    return prompt + "\n<PARSED>" + reflect.enum.get_value(parsed);
}

test "runtime_enum_renders_and_alias_round_trips_through_sap" {
    let result = main();
    assert.contains(result, "Category");
    assert.contains(result, "k7");
    assert.contains(result, "BLUE");
    assert.is_true(result.ends_with("<PARSED>RED"))
}
