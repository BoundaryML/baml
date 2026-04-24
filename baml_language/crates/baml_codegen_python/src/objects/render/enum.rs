use crate::objects::Enum;

baml_codegen_types::render_fn! {
    /// ```askama
    /// class {{enum_.name.render(crate::ty::Namespace::Types)}}(str, Enum):
    ///     {%- if let Some(docstring) = enum_.docstring %}
    ///     {{ docstring.as_docstring()|indent(4) }}
    ///     {% endif -%}
    ///
    ///     {% for variant in enum_.variants %}
    ///     {% if let Some(docstring) = variant.docstring -%}
    ///     {{ docstring.as_comment() }}
    ///     {% endif -%}
    ///     {{ variant.name }} = {{ variant.value }}
    ///     {%- endfor %}
    ///     {%- if enum_.variants.is_empty() && enum_.docstring.is_none() %}
    ///     pass
    ///     {% endif %}
    /// ```
    pub fn print(enum_: &Enum) -> String;
}
