//! Strum enums for fields that appear inside a BAML `generator` block.
//!
//! New backends or policies extend the enums here and the dispatch in
//! `baml-cli generate`.

/// Code-generation target. Surfaces as `output_type = "python/pydantic"`.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, strum::EnumString, strum::Display, strum::VariantArray,
)]
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
    /// Every supported generator target.
    pub const fn all() -> &'static [Self] {
        <Self as strum::VariantArray>::VARIANTS
    }

    /// User-facing name accepted by `baml generate add`.
    pub const fn add_name(self) -> &'static str {
        match self {
            Self::PythonPydantic => "python/pydantic2",
            Self::PythonPydanticV1 => "python/pydantic/v1",
            Self::TypescriptNode => "typescript/node",
            Self::TypescriptWeb => "typescript/web",
            Self::Swift => "swift",
            Self::Go => "go",
            Self::Rust => "rust",
            Self::Java => "java",
            Self::Cpp => "cpp",
            Self::CSharp => "csharp",
        }
    }

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

/// Manifest-ready generator settings.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Generator {
    /// Code-generation target written as `output_type`.
    pub output_type: OutputType,
    /// Optional base directory written as `output_dir`.
    pub output_dir: Option<String>,
    /// Identifier-casing policy written as `naming_convention`.
    pub naming_convention: NamingConvention,
    /// Go module import path for the generated SDK.
    pub sdk_import_path: Option<String>,
    /// Maximum typed union arity for Go output.
    pub max_typed_union_arity: Option<usize>,
    /// Whether nullable Python model fields default to `None`.
    pub nullable_fields_default_none: bool,
}

impl From<OutputType> for Generator {
    fn from(output_type: OutputType) -> Self {
        let naming_convention = match output_type {
            OutputType::Go | OutputType::CSharp => NamingConvention::Language,
            _ => NamingConvention::PreserveCase,
        };

        Self {
            output_type,
            output_dir: None,
            naming_convention,
            sdk_import_path: None,
            max_typed_union_arity: None,
            nullable_fields_default_none: false,
        }
    }
}

#[cfg(test)]
mod tests {
    use std::str::FromStr as _;

    use super::{Generator, NamingConvention, OutputType};

    #[test]
    fn csharp_generator_identity_is_canonical() {
        assert_eq!(OutputType::from_str("csharp"), Ok(OutputType::CSharp));
        assert_eq!(OutputType::CSharp.to_string(), "csharp");
        assert_eq!(OutputType::CSharp.generated_directory(), "baml_client");
        assert_eq!(OutputType::Rust.generated_directory(), "baml_sdk");
    }

    #[test]
    fn every_output_type_has_add_defaults() {
        for &output_type in OutputType::all() {
            let generator = Generator::from(output_type);
            assert_eq!(generator.output_type, output_type);
            assert!(!output_type.add_name().is_empty());
        }

        assert_eq!(
            Generator::from(OutputType::Go).naming_convention,
            NamingConvention::Language
        );
        assert_eq!(
            Generator::from(OutputType::PythonPydantic).naming_convention,
            NamingConvention::PreserveCase
        );
        assert!(!Generator::from(OutputType::PythonPydantic).nullable_fields_default_none);
    }
}
