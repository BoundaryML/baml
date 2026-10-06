// A provider request lists the tools in the order the function declares them,
// and a tool's parameters in the order its handler declares them. A provider's
// prompt cache covers the tool definitions, so the same declaration must give
// the same bytes: no client sorts, groups or deduplicates the list.
//
// Per the corpus convention every case body is a top-level `function` and the
// `test` block only asserts. Nothing here calls a model: `build_request`
// renders the request.

class ToolOrderZone {
    low: int,
    high: int,
}

enum ToolOrderKind {
    Zeta
    Alpha
}

// The names are out of alphabetical order on purpose, and so are the
// parameters of the first tool.

function tool_order_zulu(zebra: string, apple: ToolOrderZone, mango: ToolOrderKind?) -> string {
    "zulu"
}

function tool_order_alpha(query: string) -> string {
    "alpha"
}

function tool_order_mike(zones: ToolOrderZone[]) -> string {
    "mike"
}

function tool_order_bravo(count: int) -> string {
    "bravo"
}

function ToolOrderAsk(question: string) -> string {
    client: "openai/gpt-4o-mini"
    prompt: `${question}`
    tools: [tool_order_zulu, tool_order_alpha, tool_order_mike, tool_order_bravo]
}

client ToolOrderResponses = openai.ResponsesClient.new(
    model = "responses-test",
    api_key = "test-key",
    base_url = "http://localhost:1234/v1",
);

client ToolOrderChat = openai.ChatClient.new(
    model = "chat-test",
    api_key = "test-key",
    base_url = "http://localhost:1234/v1",
);

client ToolOrderAnthropic = anthropic.Client.new(
    model = "claude-test",
    api_key = "test-key",
    base_url = "http://localhost:1234",
);

client ToolOrderGemini = google.GeminiClient.new(
    model = "gemini-test",
    api_key = "test-key",
    base_url = "http://localhost:1234",
);

client ToolOrderVertex = google.VertexClient.new(
    model = "gemini-test",
    project_id = "my-project",
    location = "us-central1",
    api_key = "express-key",
);

client ToolOrderBedrock = aws.BedrockClient.new(
    model = "amazon.test",
    endpoint_url = "http://localhost:1234",
);

/// `names` in the order `body` first writes each one as a JSON string.
function tool_order_seen(body: string, names: string[]) -> string {
    let text = body;
    names
        .sort_by_key((name) -> {
            text.index_of(`"${name}"`) ?? -1
        })
        .join(",")
}

/// The order of the declared tools in the request `built_for` sends.
function tool_order_for(built_for: ai.Client) -> string {
    tool_order_seen(
        ToolOrderAsk@spec("hi").build_request(client = built_for).body,
        ["tool_order_alpha", "tool_order_bravo", "tool_order_mike", "tool_order_zulu"],
    )
}

function tool_order_all_clients() -> string[] {
    [
        tool_order_for(ToolOrderResponses),
        tool_order_for(ToolOrderChat),
        tool_order_for(ToolOrderAnthropic),
        tool_order_for(ToolOrderGemini),
        tool_order_for(ToolOrderVertex),
        tool_order_for(ToolOrderBedrock),
    ]
}

test "every provider client lists the tools in declaration order" {
    let declared = "tool_order_zulu,tool_order_alpha,tool_order_mike,tool_order_bravo";
    assert.equal(tool_order_all_clients(), [declared, declared, declared, declared, declared, declared])
}

// A toolbox built at run time is sent in the order of its list too.

function tool_order_of_a_runtime_toolbox() -> string {
    let spec = ToolOrderAsk@spec("hi");
    spec.toolbox = ai.tools.Toolbox.new(
        [
            ai.tools.tool(tool_order_bravo),
            ai.tools.tool(tool_order_mike),
            ai.tools.tool(tool_order_alpha),
            ai.tools.tool(tool_order_zulu),
        ],
    );
    tool_order_seen(
        spec.build_request(client = ToolOrderResponses).body,
        ["tool_order_alpha", "tool_order_bravo", "tool_order_mike", "tool_order_zulu"],
    )
}

test "a toolbox built at run time keeps the order of its list" {
    assert.equal(
        tool_order_of_a_runtime_toolbox(),
        "tool_order_bravo,tool_order_mike,tool_order_alpha,tool_order_zulu",
    )
}

// A tool's schema lists its parameters in the handler's parameter order.

function tool_order_parameters() -> string {
    let schema = baml.json.to_string(ai.tools.tool(tool_order_zulu).input_schema);
    tool_order_seen(schema, ["apple", "mango", "zebra"])
}

test "a tool schema lists the parameters in declaration order" {
    assert.equal(tool_order_parameters(), "zebra,apple,mango")
}

// The same declaration renders the same bytes each time.

function tool_order_renders_the_same_bytes() -> bool {
    let first = ToolOrderAsk@spec("hi").build_request(client = ToolOrderAnthropic).body;
    let second = ToolOrderAsk@spec("hi").build_request(client = ToolOrderAnthropic).body;
    first == second
}

test "the same declaration renders the same request body" {
    assert.is_true(tool_order_renders_the_same_bytes())
}
