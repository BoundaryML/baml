// The render-context surface shared by LLM function bodies and standalone
// `prompt` tag closures: the `ctx` object, chat-role markers, and the opaque
// output-format handle.

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
