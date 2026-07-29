// Run an agent without any provider tool-calling API. The model writes either
// the final Resolution or ToolCalls as text, and SAP parses that choice.
function sap_text_tool_choice() -> null {
    let provider = root
        .openai
        .responses(
            api_key = baml.env.get_or_panic("OPENAI_API_KEY"),
            output_mode = root.openai.OutputMode.Sap,
            tool_mode = root.openai.ToolMode.Prompt,
        );
    let task = _sap_resolution_task(provider, [search_knowledge]);
    let outcome = task
        .run(
            runner = root.ai.run.Agent<Resolution>.new(tools = [search_knowledge]),
        );
    log.info(outcome);
    null
}

// Show that a union return type stays intact when ToolCalls is added as
// another text choice: (SapAnswer | SapEscalation) | ToolCalls.
function sap_text_union_choice() -> null {
    let provider = root
        .openai
        .responses(
            api_key = baml.env.get_or_panic("OPENAI_API_KEY"),
            output_mode = root.openai.OutputMode.Sap,
            tool_mode = root.openai.ToolMode.Prompt,
        );
    let task = _sap_union_task(provider, [search_knowledge]);
    let outcome = task
        .run(
            runner = root.ai.run.Agent<SapSupportDecision>.new(tools = [search_knowledge]),
        );
    log.info(outcome);
    null
}
