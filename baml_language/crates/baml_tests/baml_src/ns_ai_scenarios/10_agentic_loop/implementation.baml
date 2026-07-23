class S10SearchArgs {
  query: string,
}

function s10_search_tool() -> root.ai.Tool throws never {
  root.ai.Tool.typed<S10SearchArgs>("search", "Search the documentation.")
}

function s10_dispatch(calls: root.ai.ToolCall[]) -> root.ai.ToolResult[] throws never {
  calls.map((call: root.ai.ToolCall) -> root.ai.ToolResult {
    root.ai.invoke_tool<S10SearchArgs>(call, (args: S10SearchArgs) -> string {
      `result for ${args.query}`
    })
  })
}
