//! Askama template structs for code generation.

use askama::Template;

use crate::{
    functions::FunctionRust,
    generated_types::{ClassRustRendered, EnumRust, TypeAliasRustRendered, UnionRustRendered},
    package::CurrentRenderPackage,
    r#type::SerializeType,
};

/// Source map template - embeds BAML source files
#[derive(Template)]
#[template(path = "baml_source_map.rs.j2", escape = "none")]
pub struct SourceMapTemplate<'a> {
    pub files: &'a [(String, String)],
}

/// Runtime template - FunctionOptions and singleton
#[derive(Template)]
#[template(path = "runtime.rs.j2", escape = "none")]
pub struct RuntimeTemplate<'a> {
    pub pkg: &'a CurrentRenderPackage,
}

/// Module root template
#[derive(Template)]
#[template(path = "mod.rs.j2", escape = "none")]
pub struct ModTemplate<'a> {
    pub pkg: &'a CurrentRenderPackage,
    pub prefer_async: bool,
}

/// Functions module template
#[derive(Template)]
#[template(path = "functions/mod.rs.j2", escape = "none")]
pub struct FunctionsModTemplate<'a> {
    pub functions: &'a [FunctionRust],
    pub pkg: &'a CurrentRenderPackage,
}

/// Sync client functions template
#[derive(Template)]
#[template(path = "functions/sync_client.rs.j2", escape = "none")]
pub struct FunctionsSyncClientTemplate<'a> {
    pub functions: &'a [FunctionRust],
    pub pkg: &'a CurrentRenderPackage,
}

/// Async client functions template
#[derive(Template)]
#[template(path = "functions/async_client.rs.j2", escape = "none")]
pub struct FunctionsAsyncClientTemplate<'a> {
    pub functions: &'a [FunctionRust],
    pub pkg: &'a CurrentRenderPackage,
}

/// Types module template
#[derive(Template)]
#[template(path = "types/mod.rs.j2", escape = "none")]
pub struct TypesModTemplate<'a> {
    pub classes: &'a [ClassRustRendered],
    pub enums: &'a [EnumRust],
    pub unions: &'a [UnionRustRendered],
    pub pkg: &'a CurrentRenderPackage,
}

/// Classes template
#[derive(Template)]
#[template(path = "types/classes.rs.j2", escape = "none")]
pub struct ClassesTemplate<'a> {
    pub classes: &'a [ClassRustRendered],
    pub pkg: &'a CurrentRenderPackage,
}

/// Enums template
#[derive(Template)]
#[template(path = "types/enums.rs.j2", escape = "none")]
pub struct EnumsTemplate<'a> {
    pub enums: &'a [EnumRust],
    pub pkg: &'a CurrentRenderPackage,
}

/// Unions template
#[derive(Template)]
#[template(path = "types/unions.rs.j2", escape = "none")]
pub struct UnionsTemplate<'a> {
    pub unions: &'a [UnionRustRendered],
    pub pkg: &'a CurrentRenderPackage,
}

/// Type aliases template
#[derive(Template)]
#[template(path = "types/type_aliases.rs.j2", escape = "none")]
pub struct TypeAliasesTemplate<'a> {
    pub type_aliases: &'a [TypeAliasRustRendered],
    pub pkg: &'a CurrentRenderPackage,
}

/// Stream types module template
#[derive(Template)]
#[template(path = "stream_types/mod.rs.j2", escape = "none")]
pub struct StreamTypesModTemplate<'a> {
    pub classes: &'a [ClassRustRendered],
    pub unions: &'a [UnionRustRendered],
    pub pkg: &'a CurrentRenderPackage,
}

/// Stream classes template
#[derive(Template)]
#[template(path = "stream_types/classes.rs.j2", escape = "none")]
pub struct StreamClassesTemplate<'a> {
    pub classes: &'a [ClassRustRendered],
    pub pkg: &'a CurrentRenderPackage,
}

/// Stream unions template
#[derive(Template)]
#[template(path = "stream_types/unions.rs.j2", escape = "none")]
pub struct StreamUnionsTemplate<'a> {
    pub unions: &'a [UnionRustRendered],
    pub pkg: &'a CurrentRenderPackage,
}

/// Stream type aliases template
#[derive(Template)]
#[template(path = "stream_types/type_aliases.rs.j2", escape = "none")]
pub struct StreamTypeAliasesTemplate<'a> {
    pub type_aliases: &'a [TypeAliasRustRendered],
    pub pkg: &'a CurrentRenderPackage,
}

/// Type builder module template
#[derive(Template)]
#[template(path = "type_builder/mod.rs.j2", escape = "none")]
pub struct TypeBuilderModTemplate<'a> {
    pub classes: &'a [ClassRustRendered],
    pub enums: &'a [EnumRust],
    pub pkg: &'a CurrentRenderPackage,
}

/// Type builder classes template
#[derive(Template)]
#[template(path = "type_builder/classes.rs.j2", escape = "none")]
pub struct TypeBuilderClassesTemplate<'a> {
    pub classes: &'a [ClassRustRendered],
    pub pkg: &'a CurrentRenderPackage,
}

/// Type builder enums template
#[derive(Template)]
#[template(path = "type_builder/enums.rs.j2", escape = "none")]
pub struct TypeBuilderEnumsTemplate<'a> {
    pub enums: &'a [EnumRust],
    pub pkg: &'a CurrentRenderPackage,
}
