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
  state: map<string, unknown>,
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
  reason: string,
}

class Handoff {
  to: string,
  args: json,
  transcript: Transcript,
  steps_taken: int,
}

function is_handoff(tools: Tool[], call: ToolCall) -> bool throws never {
  //# Find whether the requested tool transfers control
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

function add_usage(total: Usage, next: Usage) -> null throws never {
  total.input_tokens = total.input_tokens + next.input_tokens;
  total.output_tokens = total.output_tokens + next.output_tokens;
  total.cached_input_tokens = (total.cached_input_tokens ?? 0) + (next.cached_input_tokens ?? 0);
  total.reasoning_tokens = (total.reasoning_tokens ?? 0) + (next.reasoning_tokens ?? 0);
  total.cost_usd = (total.cost_usd ?? 0.0) + (next.cost_usd ?? 0.0);
  null
}

function run_tool_calls(
  calls: ToolCall[],
  active_tools: Tool[],
  options: AgentOptions,
  step: int,
) -> ToolResult[] throws never {
  let approved: ToolCall[] = [];
  let results: ToolResult[] = [];
  //# Review proposed tool calls
  for (let original in calls) {
    //## Publish the proposed call
    emit_agent_event(ToolCallEvent {
      id: options.run_id, phase: "proposed", call: original, result: null,
    }, options);
    //## Reject tools outside the active registry
    let known = active_tools.filter((tool: Tool) -> bool { tool.name == original.name });
    if (known.length() != 1) {
      let unavailable = ToolResult.error(original, "tool is not in the active registry");
      results.push(unavailable);
      emit_agent_event(ToolCallEvent {
        id: options.run_id,
        phase: "blocked",
        call: original,
        result: unavailable,
      }, options);
      continue;
    }
    //## Apply the before-tool hook
    let decision = match (options.hooks) {
      let hooks: AgentHooks => hooks.before_tool_call(BeforeToolCall {
        run_id: options.run_id, step: step, call: original,
      }),
      null => ToolDecision.allow(original),
    };
    //## Approve, rewrite, or block the call
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
  //# Dispatch approved tool calls
  let dispatched = options.dispatch(approved);
  //# Reconcile and publish tool results
  for (let call in approved) {
    //## Match exactly one result to the call
    let matching = dispatched.filter((candidate: ToolResult) -> bool {
      candidate.id == call.id
    });
    let result = match (matching.length()) {
      1 => matching[0],
      0 => ToolResult.error(call, "dispatcher returned no result for this tool call"),
      _ => ToolResult.error(call, "dispatcher returned duplicate results for this tool call"),
    };
    results.push(result);
    //## Apply the after-tool hook
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
  //# Select the transcript-owning provider
  // A provider transcript is authoritative continuation state. Switching its
  // owner requires explicit import; display names are not identity keys.
  let selected: Provider = match (task.transcript) {
    let existing: Transcript => existing.provider(),
    null => task.provider,
  };
  let provider = tool_calling_provider(selected);
  //# Resume or begin the provider transcript
  let transcript = match (task.transcript) {
    let existing: Transcript => existing,
    null => provider.begin<T>(task),
  };
  //# Build the active tool registry
  let registry = match (options.tool_registry) {
    let existing: ToolRegistry => existing,
    null => ToolRegistry.new(options.tools ?? task.tools),
  };
  //# Initialize cumulative usage
  let usage = Usage {
    input_tokens: 0,
    output_tokens: 0,
    cached_input_tokens: 0,
    reasoning_tokens: 0,
    cost_usd: 0.0,
  };
  let steps = 0;
  //# Run provider steps until the agent stops
  while (true) {
    //## Record the current agent state
    log.info("run_agent " + {
      "steps": steps,
      "usage": usage,
      "budget": options.budget,
      "transcript": transcript,
      "provider": provider.name(),
      "tools": registry.snapshot().map((tool: Tool) -> string { tool.name }),
    }.to_string());
    // A step is one provider.step call, including the final value-producing
    // call. Stop only between calls, so a completed tool round is always
    // submitted before a budget boundary is reported.
    //## Enforce the step budget
    match (options.budget?.max_steps) {
      let max: int if steps >= max => return BudgetReached {
        transcript: transcript,
        steps_taken: steps,
        reason: "max_steps",
      },
      _ => {},
    }
    //## Enforce the cost budget
    match (options.budget?.max_cost_usd) {
      let max: float if (usage.cost_usd ?? 0.0) >= max => return BudgetReached {
        transcript: transcript,
        steps_taken: steps,
        reason: "max_cost_usd",
      },
      _ => {},
    }
    let tools = registry.snapshot();
    let roster_version = registry.version;
    //## Prepare the next model step
    match (options.hooks) {
      let hooks: AgentHooks => {
        let plan = hooks.prepare_step(StepContext {
          run_id: options.run_id,
          step: steps,
          provider: selected,
          tools: tools,
          tool_registry: registry,
          usage: usage,
          state: options.state,
          transcript: transcript,
        });
        match (plan.stop) {
          let stop: AgentStop => return BudgetReached {
            transcript: transcript,
            steps_taken: steps,
            reason: stop.reason,
          },
          null => {},
        }
        match (plan.tools) {
          let replacement: Tool[] => registry.replace_all(replacement),
          null => {},
        }
        //### Import conversation before switching providers
        match (plan.provider) {
          let next: Provider => {
            let importer = match (next) {
              let capability: TranscriptImportProvider => capability,
              _ => throw baml.errors.Unsupported {
                message: "provider switch requires transcript import: " + next.name(),
              },
            };
            let imported = importer.import_conversation(transcript.conversation());
            let previous = selected.name();
            selected = next;
            provider = importer;
            transcript = imported.transcript;
            emit_agent_event(ProviderChangedEvent {
              id: options.run_id,
              from_provider: previous,
              to_provider: selected.name(),
              fidelity: imported.fidelity,
              warnings: imported.warnings,
            }, options);
          },
          null => {},
        }
      },
      null => {},
    }
    //## Publish changes to the active tool roster
    if (registry.version != roster_version) {
      emit_agent_event(ToolRosterChangedEvent {
        id: options.run_id,
        step: steps,
        tools: registry.snapshot().map((tool: Tool) -> string { tool.name }),
      }, options);
    }
    tools = registry.snapshot();
    emit_agent_event(ModelStartedEvent {
      id: options.run_id, provider: selected.name(),
    }, options);
    //## Ask the provider for its next step
    let step_result = provider.step<T>(transcript, tools);
    steps = steps + 1;
    //## Accumulate provider usage
    match (step_result.meta.usage) {
      let step_usage: Usage => {
        add_usage(usage, step_usage);
        emit_agent_event(UsageEvent { id: options.run_id, usage: usage }, options);
      },
      null => {},
    }
    //## Handle tool calls or a final value
    match (step_result.outcome) {
      let calls: ToolCalls => {
        //### Stop before dispatching a handoff tool
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
        //### Dispatch tools and continue the transcript
        let results = run_tool_calls(calls.calls, tools, options, steps);
        transcript = provider.submit(transcript, results);
      },
      let value: T => {
        //### Return the typed final value
        let meta = Meta {
          provider: selected.name(),
          model: null,
          request_id: options.run_id,
          finish_reason: "agent_complete",
          usage: usage,
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
    //# Run the task through the agent loop
    let outcome = run_agent<T>(task.with_provider(self.inner), AgentOptions {
      budget: self.budget,
      tools: self.tools,
      tool_registry: task.options.tool_registry,
      hooks: self.hooks ?? task.options.hooks,
      observers: task.options.observers,
      recorders: task.options.recorders,
      state: {},
      dispatch: task.options.dispatch ?? self.dispatch,
      run_id: task.options.run_id,
    }) catch (e) {
      _ => throw baml.errors.UnknownError { data: e, message: ["agent drive failed"] },
    };
    //# Require a completed typed result
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
