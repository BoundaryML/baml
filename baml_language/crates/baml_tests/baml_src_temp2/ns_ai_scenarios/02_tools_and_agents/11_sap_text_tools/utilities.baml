class SapAnswer {
    kind: "answer",
    message: string,
}

class SapEscalation {
    kind: "escalation",
    reason: string,
}

// The task's T may already be a union. Full prompt/SAP tool fallback adds
// ToolCalls as one more model-visible branch: (SapAnswer | SapEscalation)
// | ToolCalls.
type SapSupportDecision = SapAnswer | SapEscalation;

function _sap_resolution_task(
    provider: root.ai.Provider,
    tools: root.ai.tools.ToolInput[],
) -> root.ai.Task<Resolution> throws never {
    root.ai.task_named<Resolution>(provider, "SapResolution", {}, tools, (ctx: baml.llm.Context) -> baml.llm.PromptAst {
        baml.llm
            .assemble_prompt_ast(
                ["", "\n", ""],
                [
                    `
                        Call search_knowledge for "duplicate charge". After its result,
                        return a billing resolution for the customer.
                    `,
                    ctx.output_format,
                ],
            )
    })
}

function _sap_union_task(
    provider: root.ai.Provider,
    tools: root.ai.tools.ToolInput[],
) -> root.ai.Task<SapSupportDecision> throws never {
    root.ai.task_named<SapSupportDecision>(provider, "SapUnionDecision", {}, tools, (
        ctx: baml.llm.Context,
    ) -> baml.llm.PromptAst {
        baml.llm
            .assemble_prompt_ast(
                ["", "\n", ""],
                [
                    `
                        First call search_knowledge for "duplicate charge". After its
                        result, return the answer branch with kind "answer" and a short
                        customer-facing message.
                    `,
                    ctx.output_format,
                ],
            )
    })
}

function _sap_tool_call_text() -> string throws never {
    `
        {
              "calls": [{
                "id": "sap-call-1",
                "name": "search_knowledge",
                "args": {"query": "duplicate charge"}
              }]
            }
    `
}

function _sap_resolution_text() -> string throws never {
    `
        {
              "category": "billing",
              "priority": "Urgent",
              "summary": "Duplicate charge",
              "reply": "The second charge is likely a pending authorization."
            }
    `
}

function _sap_union_answer_text() -> string throws never {
    `
        {
              "kind": "answer",
              "message": "The second charge is likely a pending authorization."
            }
    `
}
