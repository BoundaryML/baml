// Prompt-rendering plumbing. These are the shared LLM render boundary that
// `ai.*` and the `prompt` tag desugar call into; no user writes them by hand,
// so they live in `ai.internal` — alongside the rest of the package's private
// helpers — while the render-context classes they build (`ai.Context`,
// `ai.Role`, `ai.OutputFormat`) sit on the public `ai` surface.

/// Reject output types that cannot be represented faithfully at the shared
/// LLM render boundary. Both prompt preview and execution call this guard.
function validate_output_type(return_type: reflect.Type) -> void throws reflect.errors.CompilationError {
    return_type._validate_renderable();
    match (return_type.as_enum()) {
        let enum_type: reflect.enum.Type => {
            if (enum_type.values().length() == 0) {
                let message = "empty enum `" + return_type.to_string() + "` cannot be rendered as an LLM output schema";
                throw reflect.errors.CompilationError {
                    message: message,
                    diagnostics: [reflect.Diagnostic {
                        code: "E0159",
                        span: null,
                        message: message,
                    }],
                }
            }
        },
        null => null,
    }
}

/// Render a return type's schema with the default output-format options.
function render_output_format(return_type: reflect.Type) -> string throws never {
    $rust_io_function
}

/// Local spelling of the public handle, so the builtin-IO extractor keeps
/// resolving the return type to `root.OutputFormat`'s generated struct.
type OutputFormat = root.OutputFormat;

/// Build the output-format handle used by `root.Context.output_format_with`.
function build_output_format(return_type: reflect.Type) -> OutputFormat throws never {
    $rust_io_function
}

/// Construct a typed role marker for structural prompt assembly.
/// The `${role(...)}` desugar in an LLM prompt body lowers to this call.
function make_role(name: string) -> root.Role throws never {
    root.Role { name: name, metadata: {} }
}

/// Look up an LLM function's declared return type. Retained for type-reflection
/// compatibility; the `ai.Agent` path uses lexical type arguments instead.
function get_return_type(function_name: string) -> reflect.Type throws baml.errors.InvalidArgument {
    $rust_io_function
}
