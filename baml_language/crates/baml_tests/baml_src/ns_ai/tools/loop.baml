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
  // Portable observation only. Exact provider continuation, when available,
  // is carried by AgentRun.session_token rather than reconstructed from this.
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
  let middleware: ToolMiddleware[] = request.options.tool_middleware ?? [];
  let observers: AgentObserver[] = request.options.observers ?? [];
  let recorders: AgentRecorder[] = request.options.recorders ?? [];
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
        let results = dispatch_with_middleware(
          calls.calls,
          dispatch,
          middleware,
          observers,
          recorders,
          ToolContext { run_id: request.options.run_id, turn: steps, provider: request.provider_name() },
        );
        publish_agent_event(
          TurnFinishedEvent { id: request.options.run_id, turn: steps }, observers, recorders,
        );
        transcript = tools_provider.submit(transcript, results);
      },
      let value: T => {
        let done = Done<T> {
          value: value,
          meta: Meta {
            provider: request.provider_name(), model: null, request_id: request.options.run_id,
            finish_reason: "tool_loop_complete", usage: null,
            attributes: { "steps": steps }, raw: null,
          },
        };
        publish_agent_event(
          RunFinishedEvent { id: request.options.run_id, outcome: "done" }, observers, recorders,
        );
        return done;
      },
    }
  }
  baml.sys.panic("unreachable")
}

function run_agent_detailed<T>(
  request: Request<T>,
  budget: Budget? = null,
) -> AgentRun<T> throws baml.errors.ToolError | baml.errors.UnknownError {
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
  let turns: AgentTurn<T>[] = [];
  let metas: Meta[] = [];
  let middleware: ToolMiddleware[] = request.options.tool_middleware ?? [];
  let observers: AgentObserver[] = request.options.observers ?? [];
  let recorders: AgentRecorder[] = request.options.recorders ?? [];
  while (true) {
    match (tools_provider.step<T>(transcript)) {
      let calls: ToolCalls => {
        let provider_step = _tool_step_meta(request.provider);
        metas.push(provider_step.meta);
        steps = steps + 1;
        match (_handoff(request.tools, calls.calls, steps)) {
          let handoff: Handoff => {
            let meta = Meta {
              provider: request.provider_name(), model: null, request_id: request.options.run_id,
              finish_reason: "handoff", usage: aggregate_usage(metas),
              attributes: { "steps": steps }, raw: null,
            };
            publish_agent_event(
              RunFinishedEvent { id: request.options.run_id, outcome: "handoff" }, observers, recorders,
            );
            return AgentRun<T> {
              done: null,
              budget_reached: null,
              handoff: handoff,
              turns: turns,
              meta: meta,
              session_token: null,
              conversation: conversation_from_messages(request.messages()),
            };
          },
          null => {},
        }
        let info = StepInfo { steps_taken: steps, cost_usd: null, calls: calls.calls };
        if (_budget_hit(budget, info)) {
          let stopped = BudgetReached { transcript: request.messages(), steps_taken: steps };
          let meta = Meta {
            provider: request.provider_name(), model: null, request_id: request.options.run_id,
            finish_reason: "budget_reached", usage: aggregate_usage(metas),
            attributes: { "steps": steps }, raw: null,
          };
          publish_agent_event(
            RunFinishedEvent { id: request.options.run_id, outcome: "budget_reached" }, observers, recorders,
          );
          return AgentRun<T> {
            done: null,
            budget_reached: stopped,
            handoff: null,
            turns: turns,
            meta: meta,
            session_token: null,
            conversation: conversation_from_messages(request.messages()),
          };
        }
        let results = dispatch_with_middleware(
          calls.calls,
          dispatch,
          middleware,
          observers,
          recorders,
          ToolContext { run_id: request.options.run_id, turn: steps, provider: request.provider_name() },
        );
        let blocks: ModelBlock[] = provider_step.blocks.slice(0, provider_step.blocks.length());
        for (let call in calls.calls) {
          blocks.push(ToolCallBlock { call: call, provider_name: request.provider_name(), metadata: null });
        }
        for (let result in results) {
          blocks.push(ToolResultBlock { result: result, provider_name: request.provider_name(), metadata: null });
        }
        turns.push(AgentTurn<T> {
          index: steps,
          blocks: blocks,
          tool_calls: calls.calls,
          tool_results: results,
          value: null,
          meta: provider_step.meta,
        });
        publish_agent_event(
          TurnFinishedEvent { id: request.options.run_id, turn: steps }, observers, recorders,
        );
        transcript = tools_provider.submit(transcript, results);
      },
      let value: T => {
        let provider_step = _tool_step_meta(request.provider);
        metas.push(provider_step.meta);
        let meta = Meta {
          provider: request.provider_name(), model: null, request_id: null,
          finish_reason: "tool_loop_complete", usage: aggregate_usage(metas),
          attributes: { "steps": steps }, raw: null,
        };
        turns.push(AgentTurn<T> {
          index: steps + 1,
          blocks: provider_step.blocks,
          tool_calls: [],
          tool_results: [],
          value: value,
          meta: provider_step.meta,
        });
        let done = Done<T> { value: value, meta: meta };
        publish_agent_event(
          RunFinishedEvent { id: request.options.run_id, outcome: "done" }, observers, recorders,
        );
        return AgentRun<T> {
          done: done,
          budget_reached: null,
          handoff: null,
          turns: turns,
          meta: meta,
          session_token: null,
          conversation: conversation_from_messages(request.messages()),
        };
      },
    }
  }
  baml.sys.panic("unreachable")
}

function _tool_step_meta(provider: Provider) -> ToolStepMeta throws never {
  match (provider) {
    let telemetry: ToolTelemetry => telemetry.last_step_meta(),
    _ => ToolStepMeta {
      blocks: [],
      meta: Meta {
        provider: provider.display_name(),
        model: null,
        request_id: null,
        finish_reason: null,
        usage: null,
        attributes: {},
        raw: null,
      },
    },
  }
}
