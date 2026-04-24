use crate::{objects::Class, ty::Namespace};

baml_codegen_types::render_fn! {
    /// ```askama
    /// class {{class_.name.render(*namespace)}}(pydantic.BaseModel):
    ///     {%- if let Some(docstring) = class_.docstring %}
    ///     {{ docstring.as_docstring()|indent(4) }}
    ///     {%- endif %}
    ///     model_config = pydantic.ConfigDict(arbitrary_types_allowed=True)
    /// {% for property in class_.properties %}
    ///     {% if let Some(docstring) = property.docstring -%}
    ///     {{ docstring.as_comment() }}
    ///     {% endif -%}
    ///     {{ property.name }}: {{ property.ty.render(*namespace) }}
    /// {%- endfor %}
    ///     {%- if class_.properties.is_empty() && class_.docstring.is_none() %}
    ///     pass
    ///     {%- endif %}
    /// ```
    pub fn print(class_: &Class, namespace: Namespace) -> String;
}
