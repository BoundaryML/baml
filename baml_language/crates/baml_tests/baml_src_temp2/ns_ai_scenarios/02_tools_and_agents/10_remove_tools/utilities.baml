function tool_roster_plan(
    context: ai.tools.StepContext,
    apply_at_step: int = 1,
    remove_name: string? = null,
    clear_roster: bool = false,
    application_tools: ai.tools.Tool[]? = null,
    next_provider: ai.Provider? = null,
) -> ai.tools.StepPlan throws never {
    if (context.step != apply_at_step) {
        return ai.tools.StepPlan { provider: null, tools: null, stop: null };
    }
    let next_tools: ai.tools.Tool[]? = match (application_tools) {
        let replacement: ai.tools.Tool[] => replacement,
        null if clear_roster => [],
        null => match (remove_name) {
            let name: string => {
                context.tools.filter((tool: ai.tools.Tool) -> bool {
                    tool.name != name
                })
            },
            null => null,
        },
    };
    ai.tools.StepPlan { provider: next_provider, tools: next_tools, stop: null }
}
