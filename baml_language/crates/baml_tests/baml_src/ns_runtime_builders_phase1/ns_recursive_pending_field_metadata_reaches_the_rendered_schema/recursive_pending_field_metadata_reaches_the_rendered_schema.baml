client TestClient = openai.ResponsesClient.new(
            model = "gpt-4o-mini",
            api_key = "test-key",
            base_url = "http://localhost:1234",
        );

function Extract<T>() -> T {
    client: TestClient
    prompt: `${ctx.output_format()}`
}

function main() -> string {
    let node = reflect.class.builder("Node");
    let next = node.type().optional();
    node.field("label", reflect.Type.of<string>());
    node.field("next", next.meta(alias = "child", description = "Recursive child"));
    let built = node.build().as_type();
    type Built = unreflect(built)
    Extract@render_prompt<Built>().text()
}

test "recursive_pending_field_metadata_reaches_the_rendered_schema" {
    let rendered = main();
    assert.contains(rendered, "child");
    assert.contains(rendered, "Recursive child");
    assert.is_true(!rendered.includes("next"))
}
