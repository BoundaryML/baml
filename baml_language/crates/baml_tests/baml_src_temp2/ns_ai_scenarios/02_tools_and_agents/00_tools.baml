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

function ResolveTicketWithTools(ticket: SupportTicket) -> Resolution {
    provider: fake_tool_provider([])
    prompt: `
        Resolve ticket ${ticket.id}. Use the available tools before answering.
        ${ctx.output_format}
    `
    tools: [search_knowledge]
}

function ToolInstructions(instructions: string) -> Resolution {
    provider: fake_tool_provider([])
    prompt: `
        ${instructions}
        ${ctx.output_format}
    `
}

class GuideObserver {
    kinds: string[],
    implements ai.observe.AgentObserver {
        function on_event(self, event: ai.observe.AgentEvent) -> null throws never {
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
    calls: ai.tools.ToolCall[],
    provider_tools: ai.tools.ProviderTool[] = [],
) -> ai.testing.FakeToolProvider throws never {
    ai.testing.FakeToolProvider {
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
    calls: ai.tools.ToolCall[],
    provider_tools: ai.tools.ProviderTool[],
) -> ai.testing.FakeToolProvider throws never {
    fake_tool_provider(calls, provider_tools = provider_tools)
}

function one_knowledge_call() -> ai.tools.ToolCall throws never {
    ai.tools.ToolCall {
        id: "call-1",
        name: "search_knowledge",
        args: { "query": "duplicate charge" },
    }
}
