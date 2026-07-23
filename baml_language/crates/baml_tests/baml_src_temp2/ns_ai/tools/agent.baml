class Budget {
  max_steps: int?,
  max_cost_usd: float?,
}

class Done<T> {
  value: T,
  meta: Meta,
  conversation: Conversation,
}

class BudgetReached {
  conversation: Conversation,
  steps_taken: int,
  reason: string,
}

class Handoff {
  to: string,
  args: json,
  conversation: Conversation,
  steps_taken: int,
}

function is_handoff(tools: Tool[], call: ToolCall) -> bool throws never {
  //# Find whether the requested tool transfers control
  for (let tool in tools) {
    if (tool.name == call.name) { return tool.handoff; }
  }
  false
}

function emit_agent_event(
  event: AgentEvent,
  hooks: AgentHooks?,
  observers: AgentObserver[],
  recorders: AgentRecorder[],
) -> null throws never {
  match (hooks) {
    let hooks: AgentHooks => hooks.on_event(event),
    null => null,
  };
  publish(event, observers, recorders)
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
  hooks: AgentHooks?,
  observers: AgentObserver[],
  recorders: AgentRecorder[],
  run_id: string?,
  step: int,
) -> ToolResult[] throws never {
  let approved: ToolCall[] = [];
  let results: ToolResult[] = [];
  //# Review proposed tool calls
  for (let original in calls) {
    //## Publish the proposed call
    emit_agent_event(ToolCallEvent {
      id: run_id, phase: "proposed", call: original, result: null,
    }, hooks, observers, recorders);
    //## Reject tools outside the active registry
    let known = active_tools.filter((tool: Tool) -> bool { tool.name == original.name });
    if (known.length() != 1) {
      let unavailable = ToolResult.error(original, "tool is not in the active registry");
      results.push(unavailable);
      emit_agent_event(ToolCallEvent {
        id: run_id,
        phase: "blocked",
        call: original,
        result: unavailable,
      }, hooks, observers, recorders);
      continue;
    }
    //## Apply the before-tool hook
    let decision = match (hooks) {
      let hooks: AgentHooks => hooks.before_tool_call(BeforeToolCall {
        run_id: run_id, step: step, call: original,
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
          id: run_id, phase: "started", call: approved_call, result: null,
        }, hooks, observers, recorders);
      },
      null => {
        let blocked = ToolResult.error(original, decision.blocked_reason ?? "blocked");
        // Provider protocols require every requested call ID to receive a
        // result. A policy block is therefore submitted as an error result,
        // not only emitted as an observability event.
        results.push(blocked);
        emit_agent_event(ToolCallEvent {
          id: run_id, phase: "blocked", call: original, result: blocked,
        }, hooks, observers, recorders);
      },
    }
  }

  //# Dispatch approved function-backed tools concurrently
  let dispatched = await baml.future.all(approved.map((call: ToolCall) -> {
    spawn {
      match (active_tools.find((tool: Tool) -> bool { tool.name == call.name })) {
        let selected: Tool => invoke_tool(selected, call),
        null => ToolResult.error(call, "rewritten tool is not in the active registry"),
      }
    }
  }));

  //# Publish one result for every approved provider call ID
  for (let result in dispatched) {
    let call = match (approved.find((candidate: ToolCall) -> bool { candidate.id == result.id })) {
      let value: ToolCall => value,
      null => baml.sys.panic("agent produced a result for an unknown tool call"),
    };
    results.push(result);
    //## Apply the after-tool hook
    match (hooks) {
      let hooks: AgentHooks => hooks.after_tool_call(AfterToolCall {
        run_id: run_id, step: step, call: call, result: result,
      }),
      null => {},
    }
    emit_agent_event(ToolCallEvent {
      id: run_id, phase: "finished", call: call, result: result,
    }, hooks, observers, recorders);
  }
  results
}

function run_agent<T>(
  task: Task<T>,
  budget: Budget?,
  tools: Tool[]?,
  tool_registry: ToolRegistry?,
  hooks: AgentHooks?,
  observers: AgentObserver[],
  recorders: AgentRecorder[],
  state: map<string, unknown>,
  run_id: string?,
  resumed: Conversation?,
) -> Done<T> | BudgetReached | Handoff
    throws root.ai.ToolError | baml.errors.UnknownError | baml.errors.Unsupported {
  //# Select the conversation-owning provider
  // A provider conversation is authoritative continuation state. Switching its
  // owner requires explicit import; display names are not identity keys.
  let selected: Provider = match (resumed) {
    let existing: Conversation => existing.provider(),
    null => task.provider,
  };
  let provider = tool_calling_provider(selected);
  //# Resume or begin the provider conversation
  let conversation = match (resumed) {
    let existing: Conversation => existing,
    null => provider.begin<T>(task),
  };
  //# Build the active tool registry
  let registry = match (tool_registry) {
    let existing: ToolRegistry => existing,
    null => ToolRegistry.new(tool_inputs_from_tools(tools ?? task.tools)),
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
      "budget": budget,
      "provider": provider.name(),
      "tools": registry.snapshot().map((tool: Tool) -> string { tool.name }),
    }.to_string());
    // A step is one provider.step call, including the final value-producing
    // call. Stop only between calls, so a completed tool round is always
    // submitted before a budget boundary is reported.
    //## Enforce the step budget
    match (budget?.max_steps) {
      let max: int if steps >= max => return BudgetReached {
        conversation: conversation,
        steps_taken: steps,
        reason: "max_steps",
      },
      _ => {},
    }
    //## Enforce the cost budget
    match (budget?.max_cost_usd) {
      let max: float if (usage.cost_usd ?? 0.0) >= max => return BudgetReached {
        conversation: conversation,
        steps_taken: steps,
        reason: "max_cost_usd",
      },
      _ => {},
    }
    let tools = registry.snapshot();
    let roster_version = registry.version;
    //## Prepare the next model step
    match (hooks) {
      let hooks: AgentHooks => {
        let plan = hooks.prepare_step(StepContext {
          run_id: run_id,
          step: steps,
          provider: selected,
          tools: tools,
          usage: usage,
          state: state,
          conversation: conversation,
        });
        match (plan.stop) {
          let stop: AgentStop => return BudgetReached {
            conversation: conversation,
            steps_taken: steps,
            reason: stop.reason,
          },
          null => {},
        }
        // A non-null plan contains the complete next roster.
        match (plan.tools) {
          let replacement: Tool[] => registry.replace_all(tool_inputs_from_tools(replacement)),
          null => {},
        }
        //### Import portable messages before switching providers
        match (plan.provider) {
          let next: Provider => {
            let importer = match (next) {
              let capability: ConversationImportProvider => capability,
              _ => throw baml.errors.Unsupported {
                message: "provider switch requires conversation import: " + next.name(),
              },
            };
            let imported = importer.import_messages(conversation.messages());
            let previous = selected.name();
            selected = next;
            provider = importer;
            conversation = imported.conversation;
            emit_agent_event(ProviderChangedEvent {
              id: run_id,
              from_provider: previous,
              to_provider: selected.name(),
              fidelity: imported.fidelity,
              warnings: imported.warnings,
            }, hooks, observers, recorders);
          },
          null => {},
        }
      },
      null => {},
    }
    //## Publish changes to the active tool roster
    if (registry.version != roster_version) {
      emit_agent_event(ToolRosterChangedEvent {
        id: run_id,
        step: steps,
        tools: registry.snapshot().map((tool: Tool) -> string { tool.name }),
      }, hooks, observers, recorders);
    }
    tools = registry.snapshot();
    emit_agent_event(ModelStartedEvent {
      id: run_id, provider: selected.name(),
    }, hooks, observers, recorders);
    //## Ask the provider for its next step
    let step_result = provider.step<T>(conversation, tools);
    steps = steps + 1;
    //## Accumulate provider usage
    match (step_result.meta.usage) {
      let step_usage: Usage => {
        add_usage(usage, step_usage);
        emit_agent_event(
          UsageEvent { id: run_id, usage: usage },
          hooks,
          observers,
          recorders,
        );
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
              id: run_id, outcome: "handoff",
            }, hooks, observers, recorders);
            return Handoff {
              to: call.name,
              args: call.args,
              conversation: conversation,
              steps_taken: steps,
            };
          }
        }
        //### Dispatch tools and continue the conversation
        let results = run_tool_calls(
          calls.calls,
          tools,
          hooks,
          observers,
          recorders,
          run_id,
          steps,
        );
        conversation = provider.submit(conversation, results);
      },
      let value: T => {
        //### Return the typed final value
        let meta = Meta {
          provider: selected.name(),
          model: null,
          request_id: run_id,
          finish_reason: "agent_complete",
          usage: usage,
          attributes: { "steps": steps },
          raw: null,
        };
        emit_agent_event(RunFinishedEvent {
          id: run_id, outcome: "done",
        }, hooks, observers, recorders);
        return Done<T> { value: value, meta: meta, conversation: conversation };
      },
    }
  }
  baml.sys.panic("unreachable")
}
