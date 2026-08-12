/// Opaque handle to a prebuilt output-format schema. This supports the
/// standalone `prompt` tag's parameterized output-format rendering.
class OutputFormat {
    _data: $rust_type,
}

/// A typed chat-role marker produced by `${role("system")}` in a prompt.
class Role {
    name: string,
    /// Provider-specific per-message metadata such as cache control.
    metadata: map<string, unknown>,
}

/// Client metadata available to a standalone `prompt` tag body.
class ContextClient {
    name: string,
    provider: string,
    default_role: string,
    allowed_roles: string[],
}

/// Render context accepted by a standalone `prompt` tag closure.
class Context {
    client: ContextClient,
    tags: map<string, unknown>,
    output_format: string,
    _output_format: OutputFormat,

    function output_format_with(
        self,
        prefix: string? = null,
        or_splitter: string? = null,
        enum_value_prefix: string? = null,
        hoisted_class_prefix: string? = null,
        always_hoist_enums: bool? = null,
        quote_class_fields: bool? = null,
        hoist_classes: string[]? = null,
        map_style: string? = null,
        render_null_as: string? = null,
    ) -> string throws never {
        $rust_io_function
    }
}

/// Render a return type's schema with the default output-format options.
function render_output_format(return_type: type) -> string throws never {
    $rust_io_function
}

/// Build the output-format handle used by `Context.output_format_with`.
function build_output_format(return_type: type) -> OutputFormat throws never {
    $rust_io_function
}

/// Construct a typed role marker for structural prompt assembly.
function make_role(name: string) -> Role throws never {
    Role { name: name, metadata: {} }
}

/// Look up an LLM function's declared return type. Retained for type-reflection
/// compatibility; the `ai.Agent` path uses lexical type arguments instead.
function get_return_type(function_name: string) -> type throws root.errors.InvalidArgument {
    $rust_io_function
}
