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
    /// Ruby SDK with Sorbet types (`baml-bridge` gem runtime).
    #[strum(serialize = "ruby/sorbet")]
    RubySorbet,
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
            Self::RubySorbet => "ruby/sorbet",
            Self::Cpp => "cpp",
            Self::CSharp => "csharp",
        }
    }

    /// Conventional generated directory for this target.
    pub const fn generated_directory(self) -> &'static str {
        "baml_sdk"
    }

    /// The naming conventions this generator implements.
    pub const fn naming_support(self) -> NamingSupport {
        match self {
            Self::Go | Self::CSharp | Self::RubySorbet => {
                NamingSupport::Only(NamingConvention::Language)
            }
            Self::Rust | Self::PythonPydantic | Self::PythonPydanticV1 => NamingSupport::Either {
                default: NamingConvention::Language,
            },
            Self::TypescriptNode | Self::TypescriptWeb | Self::Swift | Self::Java | Self::Cpp => {
                NamingSupport::Only(NamingConvention::PreserveCase)
            }
        }
    }
}

/// Identifier-casing policy a code generator must respect. Surfaces as
/// `naming_convention = "preserve-case"` or `naming_convention = "language"`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, strum::EnumString, strum::Display)]
#[strum(serialize_all = "kebab-case")]
pub enum NamingConvention {
    /// Emit BAML identifiers verbatim.
    PreserveCase,
    /// Rewrite identifiers to the target language's idiomatic casing
    /// (e.g. `snake_case` for Python functions, `PascalCase` for classes).
    Language,
}

/// Which [`NamingConvention`]s a generator implements.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NamingSupport {
    /// Only this convention; a manifest must name it.
    Only(NamingConvention),
    /// Both conventions; `baml generate add` writes `default`.
    Either { default: NamingConvention },
}

impl NamingSupport {
    /// The convention `baml generate add` writes.
    pub const fn default_convention(self) -> NamingConvention {
        match self {
            Self::Only(convention)
            | Self::Either {
                default: convention,
            } => convention,
        }
    }

    /// Whether a manifest may name `convention`.
    pub fn supports(self, convention: NamingConvention) -> bool {
        match self {
            Self::Only(only) => only == convention,
            Self::Either { .. } => true,
        }
    }
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
}

impl From<OutputType> for Generator {
    fn from(output_type: OutputType) -> Self {
        Self {
            output_type,
            output_dir: None,
            naming_convention: output_type.naming_support().default_convention(),
            sdk_import_path: None,
            max_typed_union_arity: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{Generator, NamingConvention, NamingSupport, OutputType};

    #[test]
    fn every_output_type_has_add_defaults() {
        for &output_type in OutputType::all() {
            let generator = Generator::from(output_type);
            let support = output_type.naming_support();
            assert_eq!(generator.output_type, output_type);
            assert_eq!(generator.naming_convention, support.default_convention());
            assert!(support.supports(generator.naming_convention));
            assert!(!output_type.add_name().is_empty());
        }

        for output_type in [
            OutputType::Go,
            OutputType::CSharp,
            OutputType::RubySorbet,
            OutputType::Rust,
            OutputType::PythonPydantic,
            OutputType::PythonPydanticV1,
        ] {
            assert_eq!(
                Generator::from(output_type).naming_convention,
                NamingConvention::Language
            );
        }
        assert_eq!(
            Generator::from(OutputType::TypescriptNode).naming_convention,
            NamingConvention::PreserveCase
        );
        assert_eq!(
            OutputType::Rust.naming_support(),
            NamingSupport::Either {
                default: NamingConvention::Language
            }
        );
    }
}
