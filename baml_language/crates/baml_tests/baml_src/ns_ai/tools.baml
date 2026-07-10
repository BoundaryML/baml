class Tool {
  name: string,
  description: string,
  parameters: type,
  handoff: bool?,

  function from_type(name: string, description: string, parameters: type) -> Tool throws never {
    Tool { name: name, description: description, parameters: parameters, handoff: null }
  }

  function validate_args(self, call: ToolCall) -> bool throws never {
    baml.sap.parse_type(self.parameters, call.args) catch (e) {
      _ => return false,
    };
    true
  }
}

class ToolCall {
  id: string,
  name: string,
  args: string,
}

class ToolResult {
  id: string,
  output: string,
}

class ToolCalls {
  calls: ToolCall[],
}

function invoke_tool<A>(
  call: ToolCall,
  handler: (A) -> string throws never,
) -> ToolResult throws never {
  let args: A = baml.sap.parse<A>(call.args) catch (e) {
    _ => return ToolResult {
      id: call.id,
      output: "error: arguments did not match the tool schema; fix them and retry",
    },
  };
  ToolResult { id: call.id, output: handler(args) }
}

interface Tools requires Provider {
  type Transcript
  function begin<T>(self, request: Request<T>) -> Transcript
    throws baml.errors.ToolError | baml.errors.UnknownError
  function step<T>(self, transcript: Transcript) -> T | ToolCalls
    throws baml.errors.ToolError | baml.errors.UnknownError
  function submit(self, transcript: Transcript, results: ToolResult[]) -> Transcript
    throws baml.errors.ToolError | baml.errors.UnknownError
}

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

class Agent {
  inner: Provider,
  tools: Tool[],
  dispatch: (ToolCall[]) -> ToolResult[] throws never,
  stop_when: ((StepInfo) -> bool throws never)?,

  implements Provider {
    function display_name(self) -> string throws never { "agent" }
    function render_shorthand(self) -> string throws never { self.inner.render_shorthand() }
  }

  implements Generate {
    function generate<T>(self, request: Request<T>) -> Response<T>
        throws baml.errors.CallError | baml.errors.UnknownError {
      let bound = Request<T> {
        provider: self.inner,
        prompt: request.for_provider(self.inner).prompt,
        identity: request.identity,
        arguments: request.arguments,
        tools: self.tools,
        options: RequestOptions { dispatch: self.dispatch },
        tags: request.tags,
        _render: request._render,
        _messages: request._messages,
      };
      let budget = Budget { max_steps: null, stop_when: self.stop_when };
      let outcome = run_agent<T>(bound, budget = budget) catch (e) {
        let unknown: baml.errors.UnknownError => throw unknown,
        _ => throw baml.errors.UnknownError { data: e, message: ["agent loop failed"] },
      };
      match (outcome) {
        let done: Done<T> => Response<T> { value: done.value, meta: done.meta },
        let stopped: BudgetReached => throw baml.errors.UnknownError {
          data: stopped, message: ["agent budget reached before a final value"],
        },
        let handoff: Handoff => throw baml.errors.UnknownError {
          data: handoff, message: ["agent handed off before a final value"],
        },
      }
    }
  }
}
