class S11QueryArgs {
  query: string,
}

class S11LookupArgs {
  id: string,
}

function s11_search_tool() -> root.ai.Tool throws never {
  root.ai.Tool.typed<S11QueryArgs>("search", "Search one source.")
}

function s11_lookup_tool() -> root.ai.Tool throws never {
  root.ai.Tool.typed<S11LookupArgs>("lookup", "Look up one record.")
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
