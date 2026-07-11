function invoke_tool<A>(
  call: ToolCall,
  handler: (A) -> string throws never,
) -> ToolResult throws never {
  let args: A = baml.json.from_string<A>(call.args) catch (e) {
    _ => return ToolResult {
      id: call.id,
      output: "error: arguments did not match the tool schema; fix them and retry",
    },
  };
  ToolResult { id: call.id, output: handler(args) }
}
