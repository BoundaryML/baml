test "AnyFunction tool schema distinguishes required and defaulted arguments" {
    let tool = ai.tools.tool(lookup_account_with_history);
    let properties = baml.json.path<map<string, json>>(tool.input_schema, ".properties");
    let required = baml.json.path<string[]>(tool.input_schema, ".required");

    assert.equal(tool.name, "lookup_account_with_history");
    assert.equal(tool.description, "Look up an account with optional history.");
    assert.is_true(properties.keys().includes("customer_id"));
    assert.is_true(properties.keys().includes("include_history"));
    assert.equal(required, ["customer_id"])
}

test "AnyFunction tool invocation applies ordinary function defaults" {
    let tool = ai.tools.tool(lookup_account_with_history);
    let result = ai
        .tools
        .invoke_tool(
            tool,
            ai.tools.ToolCall {
                id: "default-1",
                name: tool.name,
                args: { "customer_id": "C-1" },
            },
        );

    match (result) {
        let ok: ai.tools.ToolOk => {
            assert.equal(baml.json.path<bool>(ok.output, ".include_history"), false)
        },
        let failed: ai.tools.ToolError => assert.is_true(false),
    }
}

test "AnyFunction tool invocation returns a correlated error for invalid arguments" {
    let tool = ai.tools.tool(lookup_account_with_history);
    let result = ai
        .tools
        .invoke_tool(
            tool,
            ai.tools.ToolCall { id: "invalid-1", name: tool.name, args: { "customer_id": 42 } },
        );

    match (result) {
        let failed: ai.tools.ToolError => {
            assert.equal(failed.id, "invalid-1");
            assert.contains(failed.message, "invalid arguments")
        },
        let ok: ai.tools.ToolOk => assert.is_true(false),
    }
}

test "AnyFunction tool closure captures application state outside its schema" {
    let tenant_id = "tenant-42";
    let tool = ai
        .tools
        .tool(
            (customer_id: string) -> json {
                { "tenant_id": tenant_id, "customer_id": customer_id }
            },
            name = "tenant_lookup",
            description = "Look up an account for the captured tenant.",
        );
    let properties = baml.json.path<map<string, json>>(tool.input_schema, ".properties");
    let result = ai
        .tools
        .invoke_tool(
            tool,
            ai.tools.ToolCall {
                id: "closure-1",
                name: tool.name,
                args: { "customer_id": "C-1" },
            },
        );

    assert.equal(properties.keys(), ["customer_id"]);
    assert.equal(baml.json.path<string>(ai.tools.result_payload(result), ".tenant_id"), "tenant-42")
}

test "AnyFunction bound method excludes self from its tool schema" {
    let methods = BoundAccountTools { tenant_id: "tenant-42" };
    let tool = ai.tools.tool(methods.lookup);
    let properties = baml.json.path<map<string, json>>(tool.input_schema, ".properties");
    let result = ai
        .tools
        .invoke_tool(
            tool,
            ai.tools.ToolCall {
                id: "bound-1",
                name: tool.name,
                args: { "customer_id": "C-2" },
            },
        );

    assert.equal(tool.name, "lookup");
    assert.equal(tool.description, "Look up an account with a bound application service.");
    assert.equal(properties.keys(), ["customer_id"]);
    assert.equal(baml.json.path<string>(ai.tools.result_payload(result), ".tenant_id"), "tenant-42")
}

test "Agent.new accepts bare functions and explicit tool overrides" {
    let runner = ai
        .run
        .Agent<Resolution>
        .new(tools = [search_knowledge, ai.tools.tool(lookup_account, name = "legacy_lookup")]);
    let tools: ai.tools.Tool[] = runner.tools ?? [];

    assert
        .equal(
            tools.map((tool: ai.tools.Tool) -> string {
                tool.name
            }),
            ["search_knowledge", "legacy_lookup"],
        );
    assert.equal(tools[0].description, "Search the support knowledge base.");
    assert.equal(tools[1].description, "Look up a customer account.")
}
