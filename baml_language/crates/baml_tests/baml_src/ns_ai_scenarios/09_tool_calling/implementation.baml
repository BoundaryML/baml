class S09WeatherArgs {
  city: string,
}

class S09CompletionArgs {
  task_id: string,
  completed: bool,
}

function s09_weather_tool() -> root.ai.Tool throws never {
  root.ai.Tool.typed<S09WeatherArgs>(
    "get_weather",
    "Get the current weather for a city.",
  )
}

function s09_weather_dispatch(calls: root.ai.ToolCall[]) -> root.ai.ToolResult[] throws never {
  calls.map((call: root.ai.ToolCall) -> root.ai.ToolResult {
    root.ai.invoke_tool<S09WeatherArgs>(call, (args: S09WeatherArgs) -> string {
      `cloudy, 18 C in ${args.city}`
    })
  })
}

function s09_completion_tool() -> root.ai.Tool throws never {
  root.ai.Tool.typed<S09CompletionArgs>(
    "mark_complete",
    "Mark a task complete. Set completed to true.",
  )
}
