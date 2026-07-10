class S11QueryArgs {
  query: string,
}

function s11_search_tool() -> root.ai.Tool throws baml.errors.Unsupported {
  root.ai.Tool.from_type("search", "Search one source.", reflect.type_of<S11QueryArgs>())
}

function s11_parallel_dispatch(
  audit: string[],
) -> (root.ai.ToolCall[]) -> root.ai.ToolResult[] throws never {
  (calls: root.ai.ToolCall[]) -> root.ai.ToolResult[] {
    let futures = calls.map((call: root.ai.ToolCall) -> baml.future.Future<root.ai.ToolResult, null> {
      spawn {
        audit.push(call.id);
        root.ai.invoke_tool<S11QueryArgs>(call, (args: S11QueryArgs) -> string {
          `result for ${args.query}`
        })
      }
    });
    await baml.future.all(futures)
  }
}
