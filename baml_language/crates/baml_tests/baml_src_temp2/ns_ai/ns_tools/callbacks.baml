class StepContext {
    run_id: string?,
    step: int,
    provider: root.ai.Provider,
    tools: Tool[],
    usage: root.ai.Usage,
    state: map<string, unknown>,
    conversation: root.ai.Conversation,
}

class AgentStop {
    reason: string,
}

class StepPlan {
    provider: root.ai.Provider?,
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
