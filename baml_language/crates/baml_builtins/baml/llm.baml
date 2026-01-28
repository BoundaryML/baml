// BAML Builtin: LLM Operations
// Namespace: baml.llm
//
// This file contains the orchestrator functions for LLM operations.
// These are compiled as part of the builtin library and available
// to all BAML programs.

/// Render a prompt for an LLM function.
///
/// This is the main entry point for prompt rendering. It:
/// 1. Looks up the Jinja template for the function
/// 2. Gets the client configuration
/// 3. Renders the prompt with the given arguments
///
/// @param function_name The name of the LLM function to render
/// @param args The arguments to pass to the template
/// @param client_index Which client in the chain to use (for fallback/retry)
/// @return The rendered prompt AST
// Note: args should be map<string, any> but BAML doesn't have 'any' type yet.
// Using int as a placeholder - the actual type is determined at runtime.
function render_prompt(function_name: string, args: int, client_index: int) -> baml.llm.PromptAst {
    // Get the Jinja template string for this function
    let jinja_string = baml.llm.get_jinja_template(function_name)

    // Get the client chain for this function
    // TODO: Should return Array<ClientDefinition>, currently returns ClientCallChain
    let client_chain = baml.llm.get_client_chain(function_name)

    // Get the specific client from the chain
    let clients = client_chain.as_list()
    let client = clients.at(client_index)

    // Render the prompt using the client's configuration
    let prompt = client.render_prompt(jinja_string, args)

    return prompt
}
