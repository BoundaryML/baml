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
}

/// The attributes an enum variant can carry.
#[derive(Debug, Clone, Default, PartialEq, Eq, Hash)]
pub struct EnumVariantAttrs {
    pub schema: SchemaAttrs,
    /// `@skip`: left out of the schema and never parsed.
    pub skip: bool,
}

/// The `@@` attributes a class can carry.
#[derive(Debug, Clone, Default, PartialEq, Eq, Hash)]
pub struct ClassAttrs {
    pub schema: SchemaAttrs,
    /// `@@stream.done`: while streaming, an instance is never surfaced until
    /// the whole object is complete.
    pub stream_done: bool,
}

/// The `@@` attributes an enum can carry.
#[derive(Debug, Clone, Default, PartialEq, Eq, Hash)]
pub struct EnumAttrs {
    pub schema: SchemaAttrs,
}
