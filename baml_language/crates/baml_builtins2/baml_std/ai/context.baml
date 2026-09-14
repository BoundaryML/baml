// The render-context surface shared by LLM function bodies and standalone
// `prompt` tag closures: the `ctx` object, chat-role markers, and the opaque
// output-format handle.

/// Opaque handle to a prebuilt output-format schema. This supports the
/// standalone `prompt` tag's parameterized output-format rendering.
class OutputFormat {
    _data: $rust_type,

    function _render(
        self,
        prefix: string | null | root.internal.OutputFormatDefault = root.internal.OutputFormatDefault.Auto,
        or_splitter: string | root.internal.OutputFormatDefault = root.internal.OutputFormatDefault.Auto,
        enum_value_prefix: string | null | root.internal.OutputFormatDefault = root.internal.OutputFormatDefault.Auto,
        hoisted_class_prefix: string | null | root.internal.OutputFormatDefault = root.internal.OutputFormatDefault.Auto,
        always_hoist_enums: bool | root.internal.OutputFormatDefault = root.internal.OutputFormatDefault.Auto,
        quote_class_fields: bool | root.internal.OutputFormatDefault = root.internal.OutputFormatDefault.Auto,
        hoist_classes: bool | "auto" | string[] | root.internal.OutputFormatDefault = root.internal.OutputFormatDefault.Auto,
        map_style: "angle" | "object" | root.internal.OutputFormatDefault = root.internal.OutputFormatDefault.Auto,
        render_null_as: string | null | root.internal.OutputFormatDefault = root.internal.OutputFormatDefault.Auto,
    ) -> string throws never {
        $rust_io_function
    }
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
    _output_format: OutputFormat,

    function output_format(
        self,
        prefix: string | null | root.internal.OutputFormatDefault = root.internal.OutputFormatDefault.Auto,
        or_splitter: string | root.internal.OutputFormatDefault = root.internal.OutputFormatDefault.Auto,
        enum_value_prefix: string | null | root.internal.OutputFormatDefault = root.internal.OutputFormatDefault.Auto,
        hoisted_class_prefix: string | null | root.internal.OutputFormatDefault = root.internal.OutputFormatDefault.Auto,
        always_hoist_enums: bool | root.internal.OutputFormatDefault = root.internal.OutputFormatDefault.Auto,
        quote_class_fields: bool | root.internal.OutputFormatDefault = root.internal.OutputFormatDefault.Auto,
        hoist_classes: bool | "auto" | string[] | root.internal.OutputFormatDefault = root.internal.OutputFormatDefault.Auto,
        map_style: "angle" | "object" | root.internal.OutputFormatDefault = root.internal.OutputFormatDefault.Auto,
        render_null_as: string | null | root.internal.OutputFormatDefault = root.internal.OutputFormatDefault.Auto,
    ) -> string throws never {
        self._output_format._render(
            prefix = prefix,
            or_splitter = or_splitter,
            enum_value_prefix = enum_value_prefix,
            hoisted_class_prefix = hoisted_class_prefix,
            always_hoist_enums = always_hoist_enums,
            quote_class_fields = quote_class_fields,
            hoist_classes = hoist_classes,
            map_style = map_style,
            render_null_as = render_null_as,
        )
    }
}
