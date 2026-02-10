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
/// 4. Specializes the prompt for the client's provider
///
/// @param function_name The name of the LLM function to render
/// @param args The arguments to pass to the template
/// @return The rendered and specialized prompt AST
function render_prompt(function_name: string, args: map<string, unknown>) -> baml.llm.PromptAst {
    let jinja_string = baml.llm.get_jinja_template(function_name);

    let resolve_client_fn = baml.llm.get_client_function(function_name);
    let primitive_client = resolve_client_fn();

    let prompt = primitive_client.render_prompt(jinja_string, args);
    return primitive_client.specialize_prompt(prompt);
}

/// Build an HTTP request for an LLM function.
///
/// This renders the prompt and builds a provider-specific HTTP request
/// that can be inspected or sent manually.
///
/// @param function_name The name of the LLM function
/// @param args The arguments to pass to the template
/// @return The HTTP request ready to be sent
function build_request(function_name: string, args: map<string, unknown>) -> baml.http.Request {
    let jinja_string = baml.llm.get_jinja_template(function_name);

    let resolve_client_fn = baml.llm.get_client_function(function_name);
    let primitive_client = resolve_client_fn();

    // Render and specialize the prompt
    let prompt = primitive_client.render_prompt(jinja_string, args);
    let specialized_prompt = primitive_client.specialize_prompt(prompt);

    // Build the HTTP request
    return primitive_client.build_request(specialized_prompt);
}

/// Call an LLM function end-to-end.
///
/// This is the main entry point for executing an LLM function. It:
/// 1. Looks up the Jinja template for the function
/// 2. Gets the client's resolve function and calls it to get a PrimitiveClient
/// 3. Renders the prompt with the given arguments
/// 4. Specializes the prompt for the client's provider
/// 5. Builds an HTTP request from the prompt
/// 6. Sends the HTTP request
/// 7. Parses the response into a BAML value
///
/// @param function_name The name of the LLM function to call
/// @param args The arguments to pass to the template
/// @return The parsed response value (type is `unknown` so callers get their declared return type)
function call_llm_function(function_name: string, args: map<string, unknown>) -> unknown {
    let jinja_string = baml.llm.get_jinja_template(function_name);

    let resolve_client_fn = baml.llm.get_client_function(function_name);
    let primitive_client = resolve_client_fn();

    // Render and specialize the prompt
    let prompt = primitive_client.render_prompt(jinja_string, args);
    let specialized_prompt = primitive_client.specialize_prompt(prompt);

    // Build and send the HTTP request
    let http_request = primitive_client.build_request(specialized_prompt);
    let http_response = baml.http.send(http_request);
    let result = http_response.text();

    // Parse the response
    return primitive_client.parse(result, function_name);
}
