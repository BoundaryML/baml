class StepContext {
  run_id: string?,
  step: int,
  provider: Provider,
  tools: Tool[],
  usage: Usage,
  state: map<string, unknown>,
  conversation: Conversation,
}

class AgentStop {
  reason: string,
}

class StepPlan {
  provider: Provider?,
  // A complete replacement roster. null keeps the current roster.
  tools: Tool[]?,
  stop: AgentStop?,
}

class BeforeToolCall {
  run_id: string?,
  step: int,
  call: ToolCall,
}

class AfterToolCall {
  run_id: string?,
  step: int,
  call: ToolCall,
  result: ToolResult,
}

class ToolDecision {
  call: ToolCall?,
  blocked_reason: string?,

  function allow(call: ToolCall) -> ToolDecision throws never {
    ToolDecision { call: call, blocked_reason: null }
  }

  function replace(call: ToolCall) -> ToolDecision throws never {
    ToolDecision { call: call, blocked_reason: null }
  }

  function block(reason: string) -> ToolDecision throws never {
    ToolDecision { call: null, blocked_reason: reason }
  }
}

interface AgentHooks {
  function prepare_step(self, context: StepContext) -> StepPlan throws baml.errors.ToolError
  function before_tool_call(self, event: BeforeToolCall) -> ToolDecision throws never {
    ToolDecision.allow(event.call)
  }
  function after_tool_call(self, event: AfterToolCall) -> null throws never { null }
  function on_event(self, event: AgentEvent) -> null throws never { null }
}
