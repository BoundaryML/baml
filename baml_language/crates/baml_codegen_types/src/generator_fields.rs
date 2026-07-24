//! Strum enums for fields that appear inside a BAML `generator` block.
//!
//! Each field is required — there are deliberately no defaults while we
//! settle on per-target rules. New backends or policies extend the enums
//! here and the dispatch in `baml-cli generate`.

/// Code-generation target. Surfaces as `output_type = "python/pydantic"`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, strum::EnumString, strum::Display)]
pub enum OutputType {
    /// Python with Pydantic v2 models.
    #[strum(serialize = "python/pydantic")]
    PythonPydantic,
    /// Python with Pydantic v1 models.
    #[strum(serialize = "python/pydantic/v1")]
    PythonPydanticV1,
    /// TypeScript + Node.js SDK (`@boundaryml/baml-bridge` runtime).
    #[strum(serialize = "typescript/node")]
    TypescriptNode,
    /// Swift SDK (`BamlBridge` `SwiftPM` runtime).
    #[strum(serialize = "swift")]
    Swift,
    /// Go SDK using the `baml_go` runtime.
    #[strum(serialize = "go")]
    Go,
    /// Rust SDK (`bridge_rust` runtime crate, library name `baml_rs`).
    #[strum(serialize = "rust")]
    Rust,
    /// TypeScript + Web/WASM SDK (`@boundaryml/baml-bridge-web` runtime).
    #[strum(serialize = "typescript/web")]
    TypescriptWeb,
    /// Java SDK (`com.boundaryml:baml-bridge` runtime).
    #[strum(serialize = "java")]
    Java,
    /// C++17 SDK (self-contained source tree; dlopens the shared runtime).
    #[strum(serialize = "cpp")]
    Cpp,
    /// C# SDK compiled directly into an existing .NET project.
    #[strum(serialize = "csharp")]
    CSharp,
}

impl OutputType {
    /// Conventional generated directory for this target.
    pub const fn generated_directory(self) -> &'static str {
        match self {
            Self::CSharp => "baml_client",
            _ => "baml_sdk",
        }
    }
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

#[cfg(test)]
mod tests {
    use std::str::FromStr as _;

    use super::OutputType;

    #[test]
    fn csharp_generator_identity_is_canonical() {
        assert_eq!(OutputType::from_str("csharp"), Ok(OutputType::CSharp));
        assert_eq!(OutputType::CSharp.to_string(), "csharp");
        assert_eq!(OutputType::CSharp.generated_directory(), "baml_client");
        assert_eq!(OutputType::Rust.generated_directory(), "baml_sdk");
    }
}
