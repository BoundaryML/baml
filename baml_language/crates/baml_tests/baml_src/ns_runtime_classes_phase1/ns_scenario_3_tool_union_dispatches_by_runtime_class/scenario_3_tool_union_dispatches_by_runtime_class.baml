client TestClient = openai.ResponsesClient.new(
    model = "gpt-4o-mini",
    api_key = "test-key",
    base_url = "http://localhost:1234",
);

function PickAction<T>(context: string) -> T {
    client: TestClient
    prompt: `Pick one action for ${context}.\n${ctx.output_format()}`
}

function main() -> string {
    let read_args_t = reflect.class.new("Tool0Args", { "path": reflect.Type.of<string>().meta(alias = "file_path") });
    let read_t = reflect.class.new(
        "Tool0Action",
        {
            "tool": reflect.literal.new("filesystem/read_file").as_type(),
            "args": read_args_t.as_type(),
        },
    );
    let search_args_t = reflect.class.new(
        "Tool1Args",
        { "query": reflect.Type.of<string>(), "limit": reflect.Type.of<int>().optional().as_type() },
    );
    let search_t = reflect.class.new(
        "Tool1Action",
        { "tool": reflect.literal.new("web/search").as_type(), "args": search_args_t.as_type() },
    );
    let action_t = reflect.union.new([read_t.as_type(), search_t.as_type()]);
    type Action = unreflect(action_t.as_type())
    type Read = unreflect(read_t.as_type())
    type Search = unreflect(search_t.as_type())
    let prompt = PickAction@render_prompt<Action>("read the file").text();
    let action = PickAction@parse<Action>(`{"tool": "filesystem/read_file", "args": {"file_path": "/tmp/a.txt"}`);

    let branch = "none";
    if (action is Read) {
        branch = "read"
    } else if (action is Search) {
        branch = "search"
    }
    let matched = match (action) {
        Read => "read",
        Search => "search",
        _ => "none",
    };
    let args = reflect.class.get_field<unknown>(action, "args");
    let path = reflect.class.get_field<string>(args, "path");
    let tool = reflect.class.get_field<string>(action, "tool");
    return prompt
        + "\n<RESULT>"
        + branch
        + "|"
        + matched
        + "|"
        + (reflect.Type.of_value(action) == read_t.as_type()).to_string()
        + "|"
        + tool
        + "|"
        + path
        + "|"
        + baml.json.to_string(action);
}

test "scenario_3_tool_union_dispatches_by_runtime_class" {
    let result = main();
    assert.contains(result, "filesystem/read_file");
    assert.contains(result, "web/search");
    assert.is_true(
        result.ends_with(
            "<RESULT>read|read|true|filesystem/read_file|/tmp/a.txt|{\"tool\":\"filesystem/read_file\",\"args\":{\"path\":\"/tmp/a.txt\"}}",
        ),
    )
}
