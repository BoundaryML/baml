// Offline tests for mcp: a fake stdio MCP server written in shell
// answers the handshake, the catalog, and one call, so the connection is
// tested end-to-end with no network and no real server install.

// The fake counts id-bearing requests to mirror the client's sequential
// ids (initialize=1, tools/list=2, tools/call=3); notifications carry no
// id and are skipped.
function fake_mcp_script() -> string {
    `n=0
while IFS= read -r line; do
  case "$line" in
    *'"id":'*) n=$((n+1)) ;;
    *) continue ;;
  esac
  case "$line" in
    *'tools/list'*) printf '{"jsonrpc":"2.0","id":%s,"result":{"tools":[{"name":"echo","description":"Echo a message.","inputSchema":{"type":"object","properties":{"message":{"type":"string"}},"required":["message"]}}]}}\n' "$n" ;;
    *'tools/call'*) printf '{"jsonrpc":"2.0","id":%s,"result":{"content":[{"type":"text","text":"Echo: from-fake"}],"isError":false}}\n' "$n" ;;
    *'initialize'*) printf '{"jsonrpc":"2.0","id":%s,"result":{"protocolVersion":"2025-06-18","capabilities":{},"serverInfo":{"name":"fake","version":"0"}}}\n' "$n" ;;
  esac
done`
}

test "rpc_line frames requests and notifications" {
    let params: map<string, unknown> = {};
    let _ = params.set("a", 1);
    let req = ai.mcp.rpc_line(7, "tools/list", params);
    assert.contains(req, `"id":7`);
    assert.contains(req, `"method":"tools/list"`);
    let note = ai.mcp.rpc_line(null, "notifications/initialized", null);
    assert.is_true(!note.includes(`"id"`))
}

test "the connection speaks MCP against a fake stdio server" {
    let conn = ai.mcp.McpConnection.connect("fake", "sh", ["-c", fake_mcp_script()]);
    let tools = conn.tools();
    assert.equal(tools.length(), 1);
    if let t: ai.Tool = tools.at(0) {
        assert.equal(t.name, "echo");
        assert.contains(baml.json.stringify(baml.json.to_json(t.input_schema)), "message");
        // dispatch through the ai.Tool boundary, exactly as the runner would
        let args: map<string, unknown> = {};
        let _ = args.set("message", "hi");
        assert.contains(t.call(args), "Echo: from-fake")
    } else {
        assert.is_true(false)
    };
    conn.close()
}

class EchoBack {
    reply: string,
}

function echo_spec(c: ai.Client, tools: ai.Tool[]) -> ai.FunctionSpec<EchoBack> {
    let args: map<string, unknown> = {};
    ai.FunctionSpec {
        spec_name: "EchoBack",
        args: args,
        prompt_template: ai.Prompt {
            template: (output_format: string) -> {
                `Call the echo tool with the message 'baml-mcp-ok', then return its exact reply. ${output_format}`
            },
        },
        toolbox: ai.Toolbox.new(tools),
        default_client: c,
    }
}

test "MCP calls are journaled like any tool call" {
    let conn = ai.mcp.McpConnection.connect("fake", "sh", ["-c", fake_mcp_script()]);
    let call_args: map<string, unknown> = {};
    let _ = call_args.set("message", "hi");
    let scripted = ScriptedClient.new(
        [
            ai.ModelTurn {
                content: [ai.ToolUse { id: "m1", name: "echo", args: call_args }],
                stop_reason: ai.StopReason.ToolUse,
                usage: null,
            },
            ai.ModelTurn {
                content: [ai.Text { text: `{"reply": "Echo: from-fake"}` }],
                stop_reason: ai.StopReason.Complete,
                usage: null,
            },
        ],
    );
    let result = ai.Agent<EchoBack>.new().run(echo_spec(scripted, conn.tools()));
    conn.close();
    assert.equal(result.value.reply, "Echo: from-fake");
    // the MCP call is on the record: ToolRequested and a correlated result
    let completed = result.journal.entries().some((e: ai.Event) -> {
        match (e) {
            let c: ai.ToolCompleted => c.id == "m1" && c.output.includes("Echo: from-fake"),
            _ => false,
        }
    });
    assert.is_true(completed)
}
