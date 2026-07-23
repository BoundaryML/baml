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

function dispatch_with_middleware(
  calls: ToolCall[],
  dispatch: (ToolCall[]) -> ToolResult[] throws never,
  middleware: ToolMiddleware[],
  observers: AgentObserver[],
  recorders: AgentRecorder[],
  context: ToolContext,
) -> ToolResult[] throws never {
  let approved: ToolCall[] = [];
  let denied: ToolResult[] = [];
  for (let call in calls) {
    publish_agent_event(ToolCallProposedEvent { id: context.run_id, call: call }, observers, recorders);
    let effective: ToolCall? = call;
    let reason: string? = null;
    for (let layer in middleware) {
      match (effective) {
        null => {},
        let current: ToolCall => {
          let decision = layer.before_tool(context, current);
          effective = match (decision.call) {
            null => null,
            let rewritten: ToolCall => ToolCall {
              id: call.id,
              name: rewritten.name,
              args: rewritten.args,
            },
          };
          if (decision.call == null) { reason = decision.reason ?? "blocked by tool middleware"; }
        },
      }
    }
    match (effective) {
      null => {
        let message = reason;
        denied.push(ToolResult { id: call.id, output: "DENIED: " + message });
        publish_agent_event(
          ToolCallBlockedEvent { id: context.run_id, call: call, reason: message },
          observers,
          recorders,
        );
      },
      let current: ToolCall => {
        approved.push(current);
        publish_agent_event(ToolCallStartedEvent { id: context.run_id, call: current }, observers, recorders);
      },
    }
  }

  // The application dispatcher receives one batch, preserving its ability to
  // run independent calls concurrently.
  let dispatched = dispatch(approved);
  calls.map((original: ToolCall) -> ToolResult {
    match (denied.filter((result: ToolResult) -> bool { result.id == original.id }).at(0)) {
      let blocked: ToolResult => blocked,
      null => {
        let effective = approved.filter((call: ToolCall) -> bool { call.id == original.id }).at(0)
          ?? original;
        let result = dispatched.filter((item: ToolResult) -> bool { item.id == original.id }).at(0)
          ?? ToolResult.error(original, "dispatcher returned no correlated result");
        for (let layer in middleware) {
          result = layer.after_tool(context, effective, result);
        }
        publish_agent_event(
          ToolCallFinishedEvent { id: context.run_id, call: effective, result: result },
          observers,
          recorders,
        );
        result
      },
    }
  })
}
