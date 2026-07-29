function tool_roster_plan(
    context: root.ai.tools.StepContext,
    apply_at_step: int = 1,
    remove_name: string? = null,
    clear_roster: bool = false,
    application_tools: root.ai.tools.Tool[]? = null,
    next_provider: root.ai.Provider? = null,
) -> root.ai.tools.StepPlan throws never {
    if (context.step != apply_at_step) {
        return root.ai.tools.StepPlan { provider: null, tools: null, stop: null };
    }
    let next_tools: root.ai.tools.Tool[]? = match (application_tools) {
        let replacement: root.ai.tools.Tool[] => replacement,
        null if clear_roster => [],
        null => match (remove_name) {
            let name: string => {
                context.tools.filter((tool: root.ai.tools.Tool) -> bool {
                    tool.name != name
                })
            },
            null => null,
        },
    };
    root.ai.tools.StepPlan { provider: next_provider, tools: next_tools, stop: null }
}
