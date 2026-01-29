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
/// 2. Gets the client's resolve function and calls it to get a PrimitiveClient
/// 3. Renders the prompt with the given arguments
///
/// @param function_name The name of the LLM function to render
/// @param args The arguments to pass to the template
/// @return The rendered prompt AST
// Note: args should be map<string, any> but BAML doesn't have 'any' type yet.
// Using map<string, string> as a placeholder - the actual type is determined at runtime.
function render_prompt(function_name: string, args: map<string, string>) -> PromptAst {
    let jinja_string = baml.llm.get_jinja_template(function_name);

    let resolve_client_fn = baml.llm.get_client_function(function_name);
    let primitive_client = resolve_client_fn();

    return primitive_client.render_prompt(jinja_string, args);
}
