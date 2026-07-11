class StepInfo {
  steps_taken: int,
  cost_usd: float?,
  calls: ToolCall[],
}

class Budget {
  max_steps: int?,
  stop_when: ((StepInfo) -> bool throws never)?,
}

class Done<T> {
  value: T,
  meta: Meta,
}

class BudgetReached {
  transcript: ChatMessage[],
  steps_taken: int,
}

class Handoff {
  to: string,
  args: string,
  steps_taken: int,
}

function _handoff(tools: Tool[], calls: ToolCall[], steps: int) -> Handoff? throws never {
  for (let call in calls) {
    for (let tool in tools) {
      if (tool.name == call.name && (tool.handoff ?? false)) {
        return Handoff { to: call.name, args: call.args, steps_taken: steps };
      }
    }
  }
  null
}

function _budget_hit(budget: Budget?, info: StepInfo) -> bool throws never {
  match (budget) {
    null => false,
    let policy: Budget => {
      match (policy.max_steps) {
        let max: int => if (info.steps_taken >= max) { return true; },
        null => {},
      }
      match (policy.stop_when) {
        let stop: (StepInfo) -> bool => stop(info),
        null => false,
      }
    },
  }
}

function run_agent<T>(
  request: Request<T>,
  budget: Budget? = null,
) -> Done<T> | BudgetReached | Handoff
    throws baml.errors.ToolError | baml.errors.UnknownError {
  let tools_provider = match (request.provider) {
    let provider: Tools => provider,
    _ => throw baml.errors.Unsupported {
      message: "provider cannot run tools: " + request.provider_name(),
    },
  };
  let dispatch = match (request.options.dispatch) {
    let handler: (ToolCall[]) -> ToolResult[] => handler,
    null => throw baml.errors.Unsupported {
      message: "run_agent requires RequestOptions.dispatch",
    },
  };
  let transcript = tools_provider.begin<T>(request);
  let steps = 0;
  while (true) {
    match (tools_provider.step<T>(transcript)) {
      let calls: ToolCalls => {
        steps = steps + 1;
        match (_handoff(request.tools, calls.calls, steps)) {
          let handoff: Handoff => return handoff,
          null => {},
        }
        let info = StepInfo { steps_taken: steps, cost_usd: null, calls: calls.calls };
        if (_budget_hit(budget, info)) {
          return BudgetReached { transcript: request.messages(), steps_taken: steps };
        }
        transcript = tools_provider.submit(transcript, dispatch(calls.calls));
      },
      let value: T => return Done<T> {
        value: value,
        meta: Meta {
          provider: request.provider_name(), model: null, request_id: null,
          finish_reason: "tool_loop_complete", usage: null,
          attributes: { "steps": steps }, raw: null,
        },
      },
    }
  }
  baml.sys.panic("unreachable")
}
