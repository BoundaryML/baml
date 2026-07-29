/// Search the support knowledge base.
function search_knowledge(query: string) -> json throws never {
    { "query": query, "article": "Duplicate charges are normally pending authorizations." }
}

/// Look up a customer account.
function lookup_account(customer_id: string) -> json throws never {
    { "customer_id": customer_id, "status": "active", "tier": "pro" }
}

/// Look up an account with optional history.
function lookup_account_with_history(
    customer_id: string,
    include_history: bool = false,
) -> json throws never {
    { "customer_id": customer_id, "include_history": include_history }
}

class BoundAccountTools {
    tenant_id: string,

    /// Look up an account with a bound application service.
    function lookup(self, customer_id: string) -> json throws never {
        { "tenant_id": self.tenant_id, "customer_id": customer_id }
    }
}

// Future compiler lowering:
//   ResolveTicketWithTools.task(ticket, $provider = provider)
// becomes this hand-written ResolveTicketWithTools_task companion.
function ResolveTicketWithTools_task(
    ticket: SupportTicket,
    provider: root.ai.Provider,
    tools: root.ai.tools.ToolInput[],
) -> root.ai.Task<Resolution> throws never {
    root.ai.task_named<Resolution>(provider, "ResolveTicketWithTools", { "ticket": ticket }, tools, (
        ctx: baml.llm.Context,
    ) -> baml.llm.PromptAst {
        baml.llm
            .assemble_prompt_ast(
                ["", "\n", ""],
                [
                    `Resolve ticket ${ticket.id}. Use the available tools before answering.`,
                    ctx.output_format,
                ],
            )
    })
}

function ToolInstructions_task(
    provider: root.ai.Provider,
    tools: root.ai.tools.ToolInput[],
    instructions: string,
) -> root.ai.Task<Resolution> throws never {
    root.ai.task_named<Resolution>(provider, "ToolInstructions", { "instructions": instructions }, tools, (
        ctx: baml.llm.Context,
    ) -> baml.llm.PromptAst {
        baml.llm.assemble_prompt_ast(["", "\n", ""], [instructions, ctx.output_format])
    })
}

// Future compiler lowering:
//   ResolveTicketWithTools(ticket, $provider = agent)
// becomes a completion run over ResolveTicketWithTools_task(...). A provider
// that owns the full remote lifecycle implements CompletionProvider.
function ResolveTicketWithTools_manual(
    ticket: SupportTicket,
    agent: root.ai.Provider,
    tools: root.ai.tools.Tool[],
) -> Resolution throws root.ai.Failure | baml.errors.UnknownError {
    ResolveTicketWithTools_task(ticket, agent, root.ai.tools._tool_inputs_from_tools(tools))
        .run(runner = root.ai.run.Completion<Resolution>.new())
}

class GuideObserver {
    kinds: string[],
    implements root.ai.observe.AgentObserver {
        function on_event(self, event: root.ai.observe.AgentEvent) -> null throws never {
            self.kinds.push(event.kind());
            null
        }
    }
}

class GuideAgentTrace {
    after_calls: string[],
    events: string[],
}

function guide_agent_trace() -> GuideAgentTrace throws never {
    GuideAgentTrace { after_calls: [], events: [] }
}

function fake_tool_provider(
    calls: root.ai.tools.ToolCall[],
    provider_tools: root.ai.tools.ProviderTool[] = [],
) -> root.ai.testing.FakeToolProvider throws never {
    root.ai.testing.FakeToolProvider {
        provider_name: "fake-tools",
        tool_calls: calls,
        provider_tools: provider_tools,
        final_output: `
            {
                  "category":"billing",
                  "priority":"Urgent",
                  "summary":"Resolved with tools",
                  "reply":"The duplicate is a pending authorization."
                }
        `,
    }
}

function fake_tool_provider_with_builtins(
    calls: root.ai.tools.ToolCall[],
    provider_tools: root.ai.tools.ProviderTool[],
) -> root.ai.testing.FakeToolProvider throws never {
    fake_tool_provider(calls, provider_tools = provider_tools)
}

function one_knowledge_call() -> root.ai.tools.ToolCall throws never {
    root.ai.tools.ToolCall {
        id: "call-1",
        name: "search_knowledge",
        args: { "query": "duplicate charge" },
    }
}
