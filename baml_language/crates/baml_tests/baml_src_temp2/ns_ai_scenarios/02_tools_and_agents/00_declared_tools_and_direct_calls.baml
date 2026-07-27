/// Look up the latest status of an order.
function lookup_order(order_id: string) -> string throws never {
    `Order ${order_id} is out for delivery.`
}

/// Search the support policy documents.
function search_policy(query: string) -> string throws never {
    "Orders may be replaced when they have not moved for seven days."
}

// An explicit CompletionProvider strategy used by these examples. Direct calls
// do not know or care whether this chooses one generation or a bounded loop.
class ScenarioBoundedCompletion {
    inner: root.ai.Provider,
    max_steps: int,

    implements root.ai.Provider {
        function name(self) -> string throws never {
            "bounded-completion(" + self.inner.name() + ")"
        }
        function render_shorthand(self) -> string throws never {
            self.inner.render_shorthand()
        }
    }

    implements root.ai.CompletionProvider {
        function complete<T>(
            self,
            task: root.ai.Task<T>,
        ) -> root.ai.Response<T> throws root.ai.CallError | baml.errors.UnknownError {
            let inner_task = task.with_provider(self.inner);
            //# Complete a tool-free task with one provider generation
            if (task.tools.length() == 0) {
                let generation = match (self.inner) {
                    let provider: root.ai.GenerationProvider => provider,
                    _ => {
                        throw baml.errors.UnknownError {
                            data: null,
                            message: ["bounded completion requires GenerationProvider for a tool-free task"],
                        }
                    },
                };
                return generation.generate<T>(inner_task);
            }
            //# Complete a tool-bearing task with this provider's bounded policy
            let outcome = root
                .ai
                .run_agent<T>(
                    inner_task,
                    root.ai.Budget { max_steps: self.max_steps, max_cost_usd: null },
                    null,
                    null,
                    null,
                    null,
                    null,
                    null,
                    [],
                    [],
                    {},
                    null,
                    null,
                ) catch (e) {
                let unknown: baml.errors.UnknownError => throw unknown,
                _ => {
                    throw baml.errors.UnknownError {
                        data: e,
                        message: ["bounded completion failed while running its agent strategy"],
                    }
                },
            };
            //# Fulfill complete<T> with T or one informative failure
            match (outcome) {
                let done: root.ai.Done<T> => {
                    root.ai.Response<T> {
                        value: done.value,
                        meta: done.meta,
                        conversation: done.conversation,
                    }
                },
                let stopped: root.ai.BudgetReached => {
                    throw baml.errors.UnknownError {
                        data: stopped,
                        message: [
                            "bounded completion reached " + stopped.reason
                                + " after " + stopped.steps_taken.to_string() + " steps",
                        ],
                    }
                },
                let handoff: root.ai.Handoff => {
                    throw baml.errors.UnknownError {
                        data: handoff,
                        message: ["bounded completion cannot resolve handoff to " + handoff.to],
                    }
                },
            }
        }

        function replay_policy<T>(
            self,
            task: root.ai.Task<T>,
        ) -> root.ai.ReplayPolicy throws never {
            root.ai.ReplayPolicy { kind: root.ai.ReplayKind.Never, idempotency_key: null }
        }
    }
}

function scenario_bounded_completion(
    inner: root.ai.Provider,
    max_steps: int = 8,
) -> ScenarioBoundedCompletion throws never {
    ScenarioBoundedCompletion { inner: inner, max_steps: max_steps }
}

// Future compiler lowering:
//
//   function ResolveTicketWithDeclaredTools(ticket: SupportTicket) -> Resolution {
//     client: provider
//     prompt: `
//       Help the customer with this support ticket.
//       Use the available tools when you need more information.
//       ${ticket}
//       ${ctx.output_format}
//     `
//     tools: [lookup_order, search_policy]
//   }
//
// becomes this hand-written task companion. The declaration-owned application
// tools are captured on the task and perform no model I/O.
function ResolveTicketWithDeclaredTools_task(
    ticket: SupportTicket,
    provider: root.ai.Provider,
) -> root.ai.Task<Resolution> throws never {
    root.ai.task_named<Resolution>(provider, "ResolveTicketWithDeclaredTools", { "ticket": ticket }, [lookup_order, search_policy], (
        ctx: baml.llm.Context,
    ) -> baml.llm.PromptAst {
        baml.llm
            .assemble_prompt_ast(
                ["", "\n", ""],
                [
                    `Help the customer with ticket ${ticket.id}. Call lookup_order for O-42 and search_policy for duplicate-charge policy before answering.`,
                    ctx.output_format,
                ],
            )
    })
}

// This models the compiler/runtime's direct-call projection:
//
//   ResolveTicketWithDeclaredTools(ticket) -> Resolution
//
// The direct form asks for exactly one capability: CompletionProvider. The
// provider implementation owns whether completion is one generation, a
// bounded Agent loop, or some other bounded strategy.
function finish_task_direct_manual<T>(
    task: root.ai.Task<T>,
) -> T throws root.ai.CallError | baml.errors.UnknownError | baml.errors.Unsupported {
    //# Delegate the entire completion contract to the selected provider
    match (task.provider) {
        let provider: root.ai.CompletionProvider => provider.complete<T>(task).value,
        _ => {
            throw baml.errors.Unsupported {
                message: "provider cannot complete a direct call: " + task.provider_name(),
            }
        },
    }
}

// Future compiler lowering:
//
//   ResolveTicketWithDeclaredTools(ticket, $provider = provider)
//
// becomes a task construction followed by the standard direct-call projection.
function ResolveTicketWithDeclaredTools_manual(
    ticket: SupportTicket,
    provider: root.ai.Provider,
) -> Resolution throws root.ai.CallError | baml.errors.UnknownError | baml.errors.Unsupported {
    finish_task_direct_manual<Resolution>(ResolveTicketWithDeclaredTools_task(ticket, provider))
}

class DeclaredToolsTrace {
    lines: string[],

    implements root.ai.AgentObserver {
        function on_event(self, event: root.ai.AgentEvent) -> null throws never {
            let line: string? = match (event) {
                let started: root.ai.ModelStartedEvent => {
                    `model step (${started.provider})`
                },
                let tool: root.ai.ToolCallEvent => {
                    if (tool.phase == "proposed") {
                        `-> ${tool.call.name}`
                    } else if (tool.phase == "finished") {
                        `<- ${tool.call.name}`
                    } else {
                        null
                    }
                },
                let finished: root.ai.RunFinishedEvent => {
                    `finished: ${finished.outcome}`
                },
                _ => null,
            };
            match (line) {
                let text: string => {
                    self.lines.push(text);
                    log.info(text);
                },
                null => {},
            };
            null
        }
    }
}

function declared_tools_fake_provider() -> root.ai.FakeToolProvider throws never {
    fake_tool_provider(
        [
            root.ai.ToolCall {
                id: "declared-order",
                name: "lookup_order",
                args: { "order_id": "O-42" },
            },
            root.ai.ToolCall {
                id: "declared-policy",
                name: "search_policy",
                args: { "query": "duplicate charge" },
            },
        ],
    )
}

test "function-declared tools are captured on Task and inherited by Agent" {
    let provider = declared_tools_fake_provider();
    let task = ResolveTicketWithDeclaredTools_task(sample_ticket(), provider);
    let trace = DeclaredToolsTrace { lines: [] };

    assert
        .equal(
            task.tools.map((tool: root.ai.Tool) -> string {
                tool.name
            }),
            ["lookup_order", "search_policy"],
        );

    // No tools are repeated on Agent.new(): null inherits the task declaration.
    let outcome = task
        .run(
            runner = root.ai.run.Agent<Resolution>.new(observers = [trace], run_id = "declared-tools"),
        );

    match (outcome) {
        let done: root.ai.Done<Resolution> => {
            assert.equal(done.value.category, "billing");
            match (done.conversation) {
                let conversation: root.ai.FakeToolConversation => {
                    assert
                        .equal(
                            conversation.last_tools.map((tool: root.ai.Tool) -> string {
                                tool.name
                            }),
                            ["lookup_order", "search_policy"],
                        )
                },
                _ => assert.is_true(false),
            }
        },
        _ => assert.is_true(false),
    };

    assert
        .equal(
            trace.lines,
            [
                "model step (fake-tools)",
                "-> lookup_order",
                "-> search_policy",
                "<- lookup_order",
                "<- search_policy",
                "model step (fake-tools)",
                "finished: done",
            ],
        )
}

test "manual direct call delegates declared tools to CompletionProvider" {
    let resolution = ResolveTicketWithDeclaredTools_manual(sample_ticket(), scenario_bounded_completion(declared_tools_fake_provider()));

    assert.equal(resolution.summary, "Resolved with tools")
}

test "manual direct call without declared tools delegates to CompletionProvider" {
    let resolution = finish_task_direct_manual<Resolution>(
            ResolveTicket_task(sample_ticket(), scenario_bounded_completion(fake_resolution())),
        );

    assert.equal(resolution.summary, "Duplicate charge")
}

testset "integ-test-guide-02-declared-tools-direct-call" {
    test "live OpenAI: a direct typed call executes its declared tools" {
        let resolution = ResolveTicketWithDeclaredTools_manual(sample_ticket(), scenario_bounded_completion(live_openai()));

        log.info(
            "declared-tool direct call completed: "
                + { "summary": resolution.summary, "reply": resolution.reply }.to_string(),
        );
        assert.is_true(resolution.reply.length() > 0)
    }
}
