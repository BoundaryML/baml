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

test "AnyFunction tool schema distinguishes required and defaulted arguments" {
    let tool = root.ai.tool(lookup_account_with_history);
    let properties = baml.json.path<map<string, json>>(tool.input_schema, ".properties");
    let required = baml.json.path<string[]>(tool.input_schema, ".required");

    assert.equal(tool.name, "lookup_account_with_history");
    assert.equal(tool.description, "Look up an account with optional history.");
    assert.is_true(properties.keys().includes("customer_id"));
    assert.is_true(properties.keys().includes("include_history"));
    assert.equal(required, ["customer_id"])
}

test "AnyFunction tool invocation applies ordinary function defaults" {
    let tool = root.ai.tool(lookup_account_with_history);
    let result = root
        .ai
        .invoke_tool(
            tool,
            root.ai.ToolCall { id: "default-1", name: tool.name, args: { "customer_id": "C-1" } },
        );

    assert.is_true(!result.is_error);
    assert.equal(baml.json.path<bool>(result.output, ".include_history"), false)
}

test "AnyFunction tool invocation returns a correlated error for invalid arguments" {
    let tool = root.ai.tool(lookup_account_with_history);
    let result = root
        .ai
        .invoke_tool(
            tool,
            root.ai.ToolCall { id: "invalid-1", name: tool.name, args: { "customer_id": 42 } },
        );

    assert.equal(result.id, "invalid-1");
    assert.is_true(result.is_error);
    assert.contains(baml.json.stringify(result.output), "invalid arguments")
}

test "AnyFunction tool closure captures application state outside its schema" {
    let tenant_id = "tenant-42";
    let tool = root
        .ai
        .tool(
            (customer_id: string) -> json {
                { "tenant_id": tenant_id, "customer_id": customer_id }
            },
            name = "tenant_lookup",
            description = "Look up an account for the captured tenant.",
        );
    let properties = baml.json.path<map<string, json>>(tool.input_schema, ".properties");
    let result = root
        .ai
        .invoke_tool(
            tool,
            root.ai.ToolCall { id: "closure-1", name: tool.name, args: { "customer_id": "C-1" } },
        );

    assert.equal(properties.keys(), ["customer_id"]);
    assert.equal(baml.json.path<string>(result.output, ".tenant_id"), "tenant-42")
}

test "AnyFunction bound method excludes self from its tool schema" {
    let methods = BoundAccountTools { tenant_id: "tenant-42" };
    let tool = root.ai.tool(methods.lookup);
    let properties = baml.json.path<map<string, json>>(tool.input_schema, ".properties");
    let result = root
        .ai
        .invoke_tool(
            tool,
            root.ai.ToolCall { id: "bound-1", name: tool.name, args: { "customer_id": "C-2" } },
        );

    assert.equal(tool.name, "lookup");
    assert.equal(tool.description, "Look up an account with a bound application service.");
    assert.equal(properties.keys(), ["customer_id"]);
    assert.equal(baml.json.path<string>(result.output, ".tenant_id"), "tenant-42")
}

test "Agent.new accepts bare functions and explicit tool overrides" {
    let runner = root
        .ai
        .run
        .Agent<Resolution>
        .new(tools = [search_knowledge, root.ai.tool(lookup_account, name = "legacy_lookup")]);
    let tools: root.ai.Tool[] = runner.tools ?? [];

    assert
        .equal(
            tools.map((tool: root.ai.Tool) -> string {
                tool.name
            }),
            ["search_knowledge", "legacy_lookup"],
        );
    assert.equal(tools[0].description, "Search the support knowledge base.");
    assert.equal(tools[1].description, "Look up a customer account.")
}

// Future compiler lowering:
//   ResolveTicketWithTools.task(ticket, $provider = provider)
// becomes this hand-written ResolveTicketWithTools_task companion.
function ResolveTicketWithTools_task(
    ticket: SupportTicket,
    provider: root.ai.Provider,
    tools: root.ai.ToolInput[],
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
    tools: root.ai.ToolInput[],
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
    tools: root.ai.Tool[],
) -> Resolution throws root.ai.CallError | baml.errors.UnknownError {
    ResolveTicketWithTools_task(ticket, agent, root.ai.tool_inputs_from_tools(tools))
        .run(runner = root.ai.run.Completion<Resolution>.new())
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
        function prepare_step(self, context: root.ai.StepContext) -> root.ai.StepPlan throws never {
            let next_tools: root.ai.Tool[]? = null;
            if (context.step == 0) {
                match (self.tool_to_add) {
                    let tool: root.ai.Tool => {
                        let replacement = context.tools.slice(0, context.tools.length());
                        replacement.push(tool);
                        next_tools = replacement;
                    },
                    null => {},
                }
            }
            root.ai.StepPlan {
                provider: if (context.step == 1) {
                    self.switch_after_step
                } else {
                    null
                },
                tools: next_tools,
                stop: null,
            }
        }

        function before_tool_call(
            self,
            event: root.ai.BeforeToolCall,
        ) -> root.ai.ToolDecision throws never {
            if (self.block_name == event.call.name) {
                return root.ai.ToolDecision.block("blocked by policy");
            }
            match (self.replace_name) {
                let replacement: string => {
                    root.ai
                        .ToolDecision
                        .replace(
                            root.ai.ToolCall {
                                id: event.call.id,
                                name: replacement,
                                args: event.call.args,
                            },
                        )
                },
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
