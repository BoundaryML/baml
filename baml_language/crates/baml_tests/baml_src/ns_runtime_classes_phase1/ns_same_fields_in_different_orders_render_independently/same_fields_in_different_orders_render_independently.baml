client TestClient = openai.ResponsesClient.new(
    model = "gpt-4o-mini",
    api_key = "test-key",
    base_url = "http://localhost:1234",
);

function Render<T>() -> T {
    client: TestClient
    prompt: `${ctx.output_format()}`
}

function main() -> string {
    let left = reflect.Class.new(
        "SameName",
        { "first": int.type(), "second": string.type() },
    );
    let right = reflect.Class.new(
        "SameName",
        { "second": string.type(), "first": int.type() },
    );
    type Left = unreflect(left.as_type())
    type Right = unreflect(right.as_type())
    return Render@render_prompt<Left>().text()
        + "\n<RIGHT>\n"
        + Render@render_prompt<Right>().text()
        + "\n<UNEQUAL>"
        + (left != right).to_string();
}

test "same_fields_in_different_orders_render_independently" {
    let result = main();
    let parts = result.split("\n<RIGHT>\n");
    let left = parts[0];
    let rest = parts[1].split("\n<UNEQUAL>");
    let right = rest[0];
    assert.is_true((left.index_of("first") ?? 999999) < (left.index_of("second") ?? -1));
    assert.is_true((right.index_of("second") ?? 999999) < (right.index_of("first") ?? -1));
    assert.equal(rest[1], "true")
}
