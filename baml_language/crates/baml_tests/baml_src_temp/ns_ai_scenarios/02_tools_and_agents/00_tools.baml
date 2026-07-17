function knowledge_tool() -> root.ai.Tool throws never {
  root.ai.Tool.typed<KnowledgeLookupArgs>(
    "search_knowledge",
    "Search the support knowledge base.",
  )
}

function account_tool() -> root.ai.Tool throws never {
  root.ai.Tool.typed<AccountLookupArgs>(
    "lookup_account",
    "Look up a customer account.",
  )
}

function guide_dispatch(calls: root.ai.ToolCall[]) -> root.ai.ToolResult[] throws never {
  calls.map((call: root.ai.ToolCall) -> root.ai.ToolResult {
    if (call.name == "search_knowledge") {
      let args = baml.json.from_json<KnowledgeLookupArgs>(call.args) catch (e) {
        _ => return root.ai.ToolResult.error(call, "invalid search_knowledge arguments"),
      };
      root.ai.ToolResult.ok(call, {
        "query": args.query,
        "article": "Duplicate charges are normally pending authorizations.",
      })
    } else if (call.name == "lookup_account") {
      let args = baml.json.from_json<AccountLookupArgs>(call.args) catch (e) {
        _ => return root.ai.ToolResult.error(call, "invalid lookup_account arguments"),
      };
      root.ai.ToolResult.ok(call, {
        "customer_id": args.customer_id,
        "status": "active",
        "tier": "pro",
      })
    } else {
      root.ai.ToolResult.error(call, "unknown tool")
    }
  })
}

// Future compiler lowering:
//   ResolveTicketWithTools.task(ticket, $provider = provider)
// becomes this hand-written ResolveTicketWithTools_task companion.
function ResolveTicketWithTools_task(
  ticket: SupportTicket,
  provider: root.ai.Provider,
  tools: root.ai.Tool[],
) -> root.ai.Task<Resolution> throws never {
  root.ai.task_named<Resolution>(
    provider,
    "ResolveTicketWithTools",
    { "ticket": ticket },
    tools,
    default_task_options(),
    (ctx: baml.llm.Context) -> baml.llm.PromptAst {
      baml.llm.assemble_prompt_ast(
        ["", "\n", ""],
        [
          `Resolve ticket ${ticket.id}. Use the available tools before answering.`,
          ctx.output_format,
        ],
      )
    },
  )
}

function ToolInstructions_task(
  provider: root.ai.Provider,
  tools: root.ai.Tool[],
  instructions: string,
) -> root.ai.Task<Resolution> throws never {
  root.ai.task_named<Resolution>(
    provider,
    "ToolInstructions",
    { "instructions": instructions },
    tools,
    default_task_options(),
    (ctx: baml.llm.Context) -> baml.llm.PromptAst {
      baml.llm.assemble_prompt_ast(
        ["", "\n", ""],
        [instructions, ctx.output_format],
      )
    },
  )
}

// Future compiler lowering:
//   ResolveTicketWithTools(ticket, $provider = agent)
// becomes root.ai.drivers.drive(ResolveTicketWithTools_task(...)). The selected
// provider must be a DriveProvider such as root.ai.Agent.
function ResolveTicketWithTools_manual(
  ticket: SupportTicket,
  agent: root.ai.Provider,
  tools: root.ai.Tool[],
) -> Resolution throws baml.errors.CallError | baml.errors.UnknownError {
  root.ai.drivers.drive<Resolution>(ResolveTicketWithTools_task(ticket, agent, tools))
}

function guide_agent_options(
  tools: root.ai.Tool[],
  dispatch: (root.ai.ToolCall[]) -> root.ai.ToolResult[] throws never,
  tool_registry: root.ai.ToolRegistry? = null,
  hooks: root.ai.AgentHooks? = null,
  observers: root.ai.AgentObserver[] = [],
) -> root.ai.AgentOptions throws never {
  root.ai.AgentOptions {
    budget: root.ai.Budget { max_steps: 5, max_cost_usd: null },
    tools: tools,
    tool_registry: tool_registry,
    hooks: hooks,
    observers: observers,
    recorders: [],
    state: {},
    dispatch: dispatch,
    run_id: "guide-agent",
  }
}

class GuideObserver {
  kinds: string[],
  implements root.ai.AgentObserver {
    function on_event(self, event: root.ai.AgentEvent) -> null throws never {
      self.kinds.push(event.kind());
      null
    }
  }
}

class GuideHooks {
  tool_to_add: root.ai.Tool?,
  block_name: string?,
  replace_name: string?,
  switch_after_step: root.ai.Provider?,
  after_calls: string[],
  hook_events: string[],

  implements root.ai.AgentHooks {
    function prepare_step(self, context: root.ai.StepContext) -> root.ai.StepPlan
        throws baml.errors.ToolError {
      if (context.step == 0) {
        match (self.tool_to_add) {
          let tool: root.ai.Tool => context.tool_registry.add(tool),
          null => null,
        }
      }
      root.ai.StepPlan {
        provider: if (context.step == 1) { self.switch_after_step } else { null },
        tools: null,
        stop: null,
      }
    }

    function before_tool_call(self, event: root.ai.BeforeToolCall) -> root.ai.ToolDecision throws never {
      if (self.block_name == event.call.name) {
        return root.ai.ToolDecision.block("blocked by policy");
      }
      match (self.replace_name) {
        let replacement: string => root.ai.ToolDecision.replace(root.ai.ToolCall {
          id: event.call.id,
          name: replacement,
          args: event.call.args,
        }),
        null => root.ai.ToolDecision.allow(event.call),
      }
    }

    function after_tool_call(self, event: root.ai.AfterToolCall) -> null throws never {
      self.after_calls.push(event.call.name);
      null
    }

    function on_event(self, event: root.ai.AgentEvent) -> null throws never {
      self.hook_events.push(event.kind());
      null
    }
  }
}

function no_op_hooks() -> GuideHooks throws never {
  GuideHooks {
    tool_to_add: null,
    block_name: null,
    replace_name: null,
    switch_after_step: null,
    after_calls: [],
    hook_events: [],
  }
}

function fake_tool_provider(
  calls: root.ai.ToolCall[],
  provider_tools: root.ai.ProviderTool[] = [],
) -> root.ai.FakeToolProvider throws never {
  root.ai.FakeToolProvider {
    provider_name: "fake-tools",
    tool_calls: calls,
    provider_tools: provider_tools,
    final_output: `{
      "category":"billing",
      "priority":"Urgent",
      "summary":"Resolved with tools",
      "reply":"The duplicate is a pending authorization."
    }`,
  }
}

function fake_tool_provider_with_builtins(
  calls: root.ai.ToolCall[],
  provider_tools: root.ai.ProviderTool[],
) -> root.ai.FakeToolProvider throws never {
  fake_tool_provider(calls, provider_tools = provider_tools)
}

function one_knowledge_call() -> root.ai.ToolCall throws never {
  root.ai.ToolCall {
    id: "call-1",
    name: "search_knowledge",
    args: { "query": "duplicate charge" },
  }
}
