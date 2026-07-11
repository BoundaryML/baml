class ToolContext {
  run_id: string?,
  turn: int,
  provider: string,
}

// call=null denies the invocation. A non-null call may replace arguments.
class ToolDecision {
  call: ToolCall?,
  reason: string?,

  function allow(call: ToolCall) -> ToolDecision throws never {
    ToolDecision { call: call, reason: null }
  }
  function deny(reason: string) -> ToolDecision throws never {
    ToolDecision { call: null, reason: reason }
  }
}

// Middleware owns behavioral policy and may deny/rewrite calls or replace
// results. Pure telemetry belongs in AgentObserver/AgentRecorder instead.
interface ToolMiddleware {
  function before_tool(self, context: ToolContext, call: ToolCall) -> ToolDecision throws never {
    ToolDecision.allow(call)
  }
  function after_tool(
    self,
    context: ToolContext,
    call: ToolCall,
    result: ToolResult,
  ) -> ToolResult throws never {
    result
  }
}

function dispatch_one_with_middleware(
  call: ToolCall,
  dispatch: (ToolCall[]) -> ToolResult[] throws never,
  middleware: ToolMiddleware[],
  observers: AgentObserver[],
  recorders: AgentRecorder[],
  context: ToolContext,
) -> ToolResult throws never {
  publish_agent_event(ToolCallProposedEvent { id: context.run_id, call: call }, observers, recorders);
  let effective: ToolCall? = call;
  let denial: string? = null;
  for (let layer in middleware) {
    match (effective) {
      null => {},
      let current: ToolCall => {
        let decision = layer.before_tool(context, current);
        effective = decision.call;
        if (decision.call == null) { denial = decision.reason ?? "blocked by tool middleware"; }
      },
    }
  }
  match (effective) {
    null => ToolResult { id: call.id, output: "DENIED: " + denial },
    let current: ToolCall => {
      publish_agent_event(ToolCallStartedEvent { id: context.run_id, call: current }, observers, recorders);
      let result = dispatch([current]).at(0) ?? ToolResult {
        id: current.id,
        output: "error: dispatcher returned no correlated result",
      };
      for (let layer in middleware) {
        result = layer.after_tool(context, current, result);
      }
      publish_agent_event(
        ToolCallFinishedEvent { id: context.run_id, call: current, result: result },
        observers,
        recorders,
      );
      result
    },
  }
}

function dispatch_with_middleware(
  calls: ToolCall[],
  dispatch: (ToolCall[]) -> ToolResult[] throws never,
  middleware: ToolMiddleware[],
  observers: AgentObserver[],
  recorders: AgentRecorder[],
  context: ToolContext,
) -> ToolResult[] throws never {
  calls.map((call: ToolCall) -> ToolResult {
    dispatch_one_with_middleware(call, dispatch, middleware, observers, recorders, context)
  })
}
