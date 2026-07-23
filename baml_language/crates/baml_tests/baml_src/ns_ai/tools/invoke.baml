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

function merge_tools(first: Tool[], second: Tool[]) -> Tool[] throws baml.errors.Unsupported {
  let merged = first.slice(0, first.length());
  for (let tool in second) {
    if (merged.filter((existing: Tool) -> bool { existing.name == tool.name }).length() > 0) {
      throw baml.errors.Unsupported { message: "duplicate tool name: " + tool.name };
    }
    merged.push(tool);
  }
  merged
}

function compose_tool_dispatch(
  task_tools: Tool[],
  task_dispatch: ((ToolCall[]) -> ToolResult[] throws never)?,
  agent_tools: Tool[],
  agent_dispatch: (ToolCall[]) -> ToolResult[] throws never,
) -> (ToolCall[]) -> ToolResult[] throws never {
  (calls: ToolCall[]) -> ToolResult[] {
    let agent_calls = calls.filter((call: ToolCall) -> bool {
      agent_tools.filter((tool: Tool) -> bool { tool.name == call.name }).length() > 0
    });
    let task_calls = calls.filter((call: ToolCall) -> bool {
      task_tools.filter((tool: Tool) -> bool { tool.name == call.name }).length() > 0
    });
    let agent_results: ToolResult[] = if (agent_calls.length() == 0) { [] } else {
      agent_dispatch(agent_calls)
    };
    let task_results: ToolResult[] = if (task_calls.length() == 0) { [] } else {
      match (task_dispatch) {
        let dispatch: (ToolCall[]) -> ToolResult[] throws never => dispatch(task_calls),
        null => [],
      }
    };

    // Dispatch once per owner so each dispatcher retains its batching and
    // concurrency policy, then restore the provider's original call order.
    calls.map((call: ToolCall) -> ToolResult {
      let results: ToolResult[] = if (agent_calls.filter((owned: ToolCall) -> bool {
        owned.id == call.id
      }).length() > 0) { agent_results } else { task_results };
      results.filter((result: ToolResult) -> bool { result.id == call.id }).at(0)
        ?? ToolResult.error(call, "no dispatcher returned a result for " + call.name)
    })
  }
}
