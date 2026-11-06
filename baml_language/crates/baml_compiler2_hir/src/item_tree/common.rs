/// Re-exported unchanged: a generic parameter carries only a name and its
/// bounds (themselves `ast::TypeExpr`s, as everywhere else in the `ItemTree`),
/// so there is nothing for a mirror struct to strip.
pub use ast::GenericParam;
use baml_compiler2_ast::ast;

/// The schema metadata every data declaration can carry: what the LLM is
/// told about it and the key it serializes under.
///
/// Lowered from the declaration's attributes by `crate::attrs` — the one
/// place raw attributes are read — so a value here has already been
/// validated; consumers never re-parse an attribute.
#[derive(Debug, Clone, Default, PartialEq, Eq, Hash)]
pub struct SchemaAttrs {
    /// `@description("…")`.
    pub description: Option<String>,
    /// `@alias("…")`: the serialized key, used instead of the declared name.
    pub alias: Option<String>,
}

/// The attributes a class field can carry.
// Each bool is one flag attribute.
#[allow(clippy::struct_excessive_bools)]
#[derive(Debug, Clone, Default, PartialEq, Eq, Hash)]
pub struct ClassFieldAttrs {
    pub schema: SchemaAttrs,
    /// `@skip`: left out of the schema and never parsed.
    pub skip: bool,
    /// `@stream.done`: while streaming, the field holds its default until its
    /// value is complete — an incomplete value is never surfaced.
    pub stream_done: bool,
    /// `@stream.must_exist`: the field has no default, so its class has no
    /// partial parse until the field is present.
    pub must_exist: bool,
    /// `@json.rename("…")`: the field's JSON key.
    pub json_rename: Option<String>,
    /// `@json.flatten`: the field holds every key no other field took.
    pub json_flatten: bool,
    /// `@json.fallback`: the field holds the whole object, and its class is
    /// the member of a union no other member's literals match.
    pub json_fallback: bool,
}

/// The attributes an enum variant can carry.
#[derive(Debug, Clone, Default, PartialEq, Eq, Hash)]
pub struct EnumVariantAttrs {
    pub schema: SchemaAttrs,
    /// `@skip`: left out of the schema and never parsed.
    pub skip: bool,
    /// `@json.rename("…")`: the variant's JSON string.
    pub json_rename: Option<String>,
}

/// The `@@` attributes a class can carry.
#[derive(Debug, Clone, Default, PartialEq, Eq, Hash)]
pub struct ClassAttrs {
    pub schema: SchemaAttrs,
    /// `@@stream.done`: while streaming, an instance is never surfaced until
    /// the whole object is complete.
    pub stream_done: bool,
    /// `@@json.omit_nulls`: the class's optional fields holding `null` are not
    /// written. A `json` field keeps its `null`.
    pub json_omit_nulls: bool,
}

/// The `@@` attributes an enum can carry.
#[derive(Debug, Clone, Default, PartialEq, Eq, Hash)]
pub struct EnumAttrs {
    pub schema: SchemaAttrs,
}
