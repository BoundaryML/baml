//! Strum enums for fields that appear inside a BAML `generator` block.
//!
//! Each field is required — there are deliberately no defaults while we
//! settle on per-target rules. New backends or policies extend the enums
//! here and the dispatch in `baml-cli generate`.

/// Code-generation target. Surfaces as `output_type "python/pydantic"`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, strum::EnumString, strum::Display)]
pub enum OutputType {
    /// Python with Pydantic v2 models.
    #[strum(serialize = "python/pydantic")]
    PythonPydantic,
    /// Python with Pydantic v1 models.
    #[strum(serialize = "python/pydantic/v1")]
    PythonPydanticV1,
}

/// Identifier-casing policy a code generator must respect. Surfaces as
/// `naming_convention "preserve-case"` or `naming_convention "language"`.
/// Future work will branch on this to rewrite identifiers for the target
/// language.
#[derive(Debug, Clone, Copy, PartialEq, Eq, strum::EnumString, strum::Display)]
#[strum(serialize_all = "kebab-case")]
pub enum NamingConvention {
    /// Emit BAML identifiers verbatim.
    PreserveCase,
    /// Rewrite identifiers to the target language's idiomatic casing
    /// (e.g. `snake_case` for Python functions, `PascalCase` for classes).
    Language,
}
