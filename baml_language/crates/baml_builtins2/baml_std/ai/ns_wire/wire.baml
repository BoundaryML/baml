// Shared helpers for client authors: transport with classification, the
// output-format text, and tool schema lowering.
// Mirrors _planv2/pages/02_guides/03_clients/03_writing_a_client.md.

// Send, classify non-2xx via root.errors.classify_http, decode the body as T.
// T = json-shaped envelope class; unknown fields are ignored by the decoder.
function send_as<T>(req: baml.http.Request, provider: string) -> T {
    let resp = baml.http.send(req) catch_all (e) {
        _ => {
            throw root.errors.NetworkFailure {
            provider: provider,
            detail: `transport failed before a response arrived: ${e.to_string()}`,
            raw_body: null,
        }
        },
    };
    let text = resp.text() catch_all (e) {
        _ => {
            throw root.errors.NetworkFailure {
            provider: provider,
            detail: "the response body could not be read completely",
            raw_body: null,
        }
        },
    };
    if (!resp.ok()) {
        throw root.errors.classify_http(provider, resp.status_code, text)
    }
    baml.json.from_string<T>(text) catch_all (e) {
        _ => throw root.errors.ParseFailed { provider: provider, raw_output: text },
    }
}

// The return type's schema as prompt text, in the standard dialect —
// exactly what ${ctx.output_format} renders in an LLM function block.
function render_output_format(t: type) -> string throws baml.reflect.errors.CompilationError {
    baml.prompt.validate_output_type(t);
    baml.prompt.render_output_format(t)
}
