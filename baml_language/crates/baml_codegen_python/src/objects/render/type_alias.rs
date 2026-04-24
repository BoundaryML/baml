use crate::{objects::TypeAlias, ty::Namespace};

baml_codegen_types::render_fn! {
    /// ```askama
    /// {% if type_alias.is_recursive() -%}
    /// {{ type_alias.render_name(*namespace) }} = typing_extensions.TypeAliasType("{{ type_alias.render_name(*namespace) }}", {{ type_alias.render_rhs(*namespace) }})
    /// {% else -%}
    /// {{ type_alias.render_name(*namespace) }} = {{ type_alias.render_rhs(*namespace) }}
    /// {% endif %}
    /// ```
    pub fn print(type_alias: &TypeAlias, namespace: Namespace) -> String;
}
