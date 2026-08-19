//! Strum enums for fields that appear inside a BAML `generator` block.
//!
//! New backends or policies extend the enums here and the dispatch in
//! `baml-cli generate`.

/// Code-generation target. Surfaces as `output_type = "python/pydantic"`.
///
/// [`std::fmt::Display`] is the single spelling authority: it produces the
/// canonical name written into `baml.toml`, and every canonical name parses
/// back through [`std::str::FromStr`]. The two `serialize` aliases (`python`,
/// `typescript`) are additionally accepted on input but are never written.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, strum::EnumString, strum::Display, strum::VariantArray,
)]
pub enum OutputType {
    /// Python with Pydantic v2 models. There is only one Python target, so
    /// the name carries no Pydantic version.
    #[strum(to_string = "python/pydantic", serialize = "python")]
    PythonPydantic,
    /// TypeScript + Node.js SDK (`@boundaryml/baml-bridge` runtime).
    #[strum(to_string = "typescript/node", serialize = "typescript")]
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

    /// The canonical name, as a `'static` string.
    ///
    /// Identical to [`std::fmt::Display`] (asserted by a test); this exists
    /// only for callers that need a `'static` lifetime, such as clap's
    /// `PossibleValue` (this crate does not depend on clap, so that type is
    /// named here rather than linked).
    pub const fn canonical(self) -> &'static str {
        match self {
            Self::PythonPydantic => "python/pydantic",
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

    /// Additional spellings accepted on input but never written to
    /// `baml.toml`. Surfaced as hidden clap possible values.
    pub const fn aliases(self) -> &'static [&'static str] {
        match self {
            Self::PythonPydantic => &["python"],
            Self::TypescriptNode => &["typescript"],
            _ => &[],
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

    /// `Display` writes `baml.toml`; `FromStr` reads it back. A canonical
    /// name that could not round-trip would silently corrupt the manifest.
    #[test]
    fn canonical_names_round_trip_through_display_and_from_str() {
        for &output_type in OutputType::all() {
            let displayed = output_type.to_string();
            assert_eq!(
                OutputType::from_str(&displayed),
                Ok(output_type),
                "`{displayed}` does not parse back"
            );
        }
    }

    /// `canonical()` exists only to hand `Display`'s spelling to callers
    /// needing `'static`; the two must never drift.
    #[test]
    fn canonical_matches_display() {
        for &output_type in OutputType::all() {
            assert_eq!(output_type.canonical(), output_type.to_string());
        }
    }

    #[test]
    fn aliases_parse_and_are_never_the_canonical_spelling() {
        assert_eq!(
            OutputType::from_str("python"),
            Ok(OutputType::PythonPydantic)
        );
        assert_eq!(
            OutputType::from_str("typescript"),
            Ok(OutputType::TypescriptNode)
        );
        // The deleted Pydantic-v1 target silently emitted v2 output; the
        // spelling must now be an unknown value rather than a wrong one.
        assert!(OutputType::from_str("python/pydantic/v1").is_err());
        // `generate add`'s old spelling disagreed with `baml.toml`'s.
        assert!(OutputType::from_str("python/pydantic2").is_err());

        for &output_type in OutputType::all() {
            for alias in output_type.aliases() {
                assert_eq!(OutputType::from_str(alias), Ok(output_type));
                assert_ne!(*alias, output_type.canonical());
            }
        }
    }

    #[test]
    fn every_output_type_has_add_defaults() {
        for &output_type in OutputType::all() {
            let generator = Generator::from(output_type);
            assert_eq!(generator.output_type, output_type);
            assert!(!output_type.canonical().is_empty());
        }

        assert_eq!(
            Generator::from(OutputType::Go).naming_convention,
            NamingConvention::Language
        );
        assert_eq!(
            Generator::from(OutputType::PythonPydantic).naming_convention,
            NamingConvention::PreserveCase
        );
    }
}
