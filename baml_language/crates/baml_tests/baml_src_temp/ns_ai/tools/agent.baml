class Budget {
  max_steps: int?,
  max_cost_usd: float?,
}

class AgentOptions {
  budget: Budget?,
  // null inherits task tools; [] intentionally exposes no application tools.
  tools: Tool[]?,
  tool_registry: ToolRegistry?,
  hooks: AgentHooks?,
  observers: AgentObserver[],
  recorders: AgentRecorder[],
  dispatch: (ToolCall[]) -> ToolResult[] throws never,
  run_id: string?,
}

class Done<T> {
  value: T,
  meta: Meta,
  transcript: Transcript,
}

class BudgetReached {
  transcript: Transcript,
  steps_taken: int,
}

class Handoff {
  to: string,
  args: json,
  transcript: Transcript,
  steps_taken: int,
}

function is_handoff(tools: Tool[], call: ToolCall) -> bool throws never {
  for (let tool in tools) {
    if (tool.name == call.name) { return tool.handoff; }
  }
  false
}

function emit_agent_event(event: AgentEvent, options: AgentOptions) -> null throws never {
  match (options.hooks) {
    let hooks: AgentHooks => hooks.on_event(event),
    null => null,
  };
  publish(event, options.observers, options.recorders)
}

function run_tool_calls(
  calls: ToolCall[],
  options: AgentOptions,
  step: int,
) -> ToolResult[] throws never {
  let approved: ToolCall[] = [];
  let results: ToolResult[] = [];
  for (let original in calls) {
    emit_agent_event(ToolCallEvent {
      id: options.run_id, phase: "proposed", call: original, result: null,
    }, options);
    let decision = match (options.hooks) {
      let hooks: AgentHooks => hooks.before_tool_call(BeforeToolCall {
        run_id: options.run_id, step: step, call: original,
      }),
      null => ToolDecision.allow(original),
    };
    match (decision.call) {
      let call: ToolCall => {
        // Hooks may rewrite the tool name or arguments, but the provider owns
        // the correlation ID. Normalize it back to the original ID.
        let approved_call = ToolCall {
          id: original.id,
          name: call.name,
          args: call.args,
        };
        approved.push(approved_call);
        emit_agent_event(ToolCallEvent {
          id: options.run_id, phase: "started", call: approved_call, result: null,
        }, options);
      },
      null => {
        let blocked = ToolResult.error(original, decision.blocked_reason ?? "blocked");
        // Provider protocols require every requested call ID to receive a
        // result. A policy block is therefore submitted as an error result,
        // not only emitted as an observability event.
        results.push(blocked);
        emit_agent_event(ToolCallEvent {
          id: options.run_id, phase: "blocked", call: original, result: blocked,
        }, options);
      },
    }
  }
  let dispatched = options.dispatch(approved);
  for (let call in approved) {
    let matching = dispatched.filter((candidate: ToolResult) -> bool {
      candidate.id == call.id
    });
    let result = match (matching.length()) {
      1 => matching[0],
      0 => ToolResult.error(call, "dispatcher returned no result for this tool call"),
      _ => ToolResult.error(call, "dispatcher returned duplicate results for this tool call"),
    };
    results.push(result);
    match (options.hooks) {
      let hooks: AgentHooks => hooks.after_tool_call(AfterToolCall {
        run_id: options.run_id, step: step, call: call, result: result,
      }),
      null => {},
    }
    emit_agent_event(ToolCallEvent {
      id: options.run_id, phase: "finished", call: call, result: result,
    }, options);
  }
  results
}

function run_agent<T>(task: Task<T>, options: AgentOptions) -> Done<T> | BudgetReached | Handoff
    throws baml.errors.ToolError | baml.errors.UnknownError | baml.errors.Unsupported {
  let selected: Provider = task.provider;
  let provider = match (selected) {
    let capability: ToolCallingProvider => capability,
    _ => throw baml.errors.Unsupported {
      message: "provider cannot run a tool loop: " + task.provider_name(),
    },
  };
  let transcript = provider.begin<T>(task);
  let steps = 0;
  while (true) {
    // A step is one provider.step call, including the final value-producing
    // call. Stop only between calls, so a completed tool round is always
    // submitted before a budget boundary is reported.
    match (options.budget?.max_steps) {
      let max: int if steps >= max => return BudgetReached {
        transcript: transcript,
        steps_taken: steps,
      },
      _ => {},
    }
    let tools = match (options.tool_registry) {
      let registry: ToolRegistry => registry.snapshot(),
      null => options.tools ?? task.tools,
    };
    match (options.hooks) {
      let hooks: AgentHooks => {
        let plan = hooks.prepare_step(StepContext {
          run_id: options.run_id,
          step: steps,
          provider: selected,
          tools: tools,
          transcript: transcript,
        });
        match (plan.stop) {
          let stop: AgentStop => return BudgetReached {
            transcript: transcript,
            steps_taken: steps,
          },
          null => {},
        }
        match (plan.tools) {
          let replacement: Tool[] => { tools = replacement; },
          null => {},
        }
        match (plan.provider) {
          let next: Provider => {
            let importer = match (next) {
              let capability: TranscriptImportProvider => capability,
              _ => throw baml.errors.Unsupported {
                message: "provider switch requires transcript import: " + next.name(),
              },
            };
            let imported = importer.import_conversation(transcript.conversation());
            selected = next;
            provider = importer;
            transcript = imported.transcript;
          },
          null => {},
        }
      },
      null => {},
    }
    emit_agent_event(ModelStartedEvent {
      id: options.run_id, provider: selected.name(),
    }, options);
    let step_result = provider.step<T>(transcript, tools);
    steps = steps + 1;
    match (step_result) {
      let calls: ToolCalls => {
        for (let call in calls.calls) {
          if (is_handoff(tools, call)) {
            emit_agent_event(RunFinishedEvent {
              id: options.run_id, outcome: "handoff",
            }, options);
            return Handoff {
              to: call.name,
              args: call.args,
              transcript: transcript,
              steps_taken: steps,
            };
          }
        }
        let results = run_tool_calls(calls.calls, options, steps);
        transcript = provider.submit(transcript, results);
      },
      let value: T => {
        let meta = Meta {
          provider: selected.name(),
          model: null,
          request_id: options.run_id,
          finish_reason: "agent_complete",
          usage: null,
          attributes: { "steps": steps },
          raw: null,
        };
        emit_agent_event(RunFinishedEvent {
          id: options.run_id, outcome: "done",
        }, options);
        return Done<T> { value: value, meta: meta, transcript: transcript };
      },
    }
  }
  baml.sys.panic("unreachable")
}

class Agent {
  inner: Provider,
  tools: Tool[],
  dispatch: (ToolCall[]) -> ToolResult[] throws never,
  budget: Budget?,
  hooks: AgentHooks?,
}

implements Provider for Agent {
  function name(self) -> string throws never { "agent(" + self.inner.name() + ")" }
  function render_shorthand(self) -> string throws never { self.inner.render_shorthand() }
}

implements DriveProvider for Agent {
  function drive<T>(self, task: Task<T>) -> Response<T> throws baml.errors.UnknownError {
    let outcome = run_agent<T>(task.with_provider(self.inner), AgentOptions {
      budget: self.budget,
      tools: self.tools,
      tool_registry: task.options.tool_registry,
      hooks: self.hooks ?? task.options.hooks,
      observers: task.options.observers,
      recorders: task.options.recorders,
      dispatch: task.options.dispatch ?? self.dispatch,
      run_id: task.options.run_id,
    }) catch (e) {
      _ => throw baml.errors.UnknownError { data: e, message: ["agent drive failed"] },
    };
    match (outcome) {
      let done: Done<T> => Response<T> {
        value: done.value,
        meta: done.meta,
        transcript: done.transcript,
      },
      _ => throw baml.errors.UnknownError {
        data: outcome,
        message: ["agent did not finish with a final value"],
      },
    }
  }
}
