use crate::{lockfile::LockFileWrapper, PreviewFeature};
use enumflags2::BitFlags;
use std::path::PathBuf;

#[derive(Debug)]
pub struct Configuration {
    pub generators: Vec<(Generator, LockFileWrapper)>,
}

impl Configuration {
    pub fn new() -> Self {
        Self { generators: vec![] }
    }

    pub fn preview_features(&self) -> BitFlags<PreviewFeature> {
        self.generators
            .iter()
            .fold(BitFlags::empty(), |acc, _generator| acc)
    }
}

#[derive(Debug, Clone, strum::Display, strum::EnumString, strum::VariantNames)]
pub enum GeneratorOutputType {
    #[strum(serialize = "python/pydantic")]
    PythonPydantic,
    #[strum(serialize = "typescript")]
    Typescript,
    #[strum(serialize = "ruby/sorbet")]
    RubySorbet,
}

impl GeneratorOutputType {
    pub fn default_client_mode(&self) -> GeneratorDefaultClientMode {
        match self {
            // Due to legacy reasons, PythonPydantic and Typescript default to async
            // DO NOT CHANGE THIS DEFAULT EVER OR YOU WILL BREAK EXISTING USERS
            Self::PythonPydantic => GeneratorDefaultClientMode::Async,
            Self::Typescript => GeneratorDefaultClientMode::Async,
            Self::RubySorbet => GeneratorDefaultClientMode::Sync,
        }
    }

    /// Used to new generators when they are created (e.g. during baml-cli init)
    pub fn recommended_default_client_mode(&self) -> GeneratorDefaultClientMode {
        match self {
            Self::PythonPydantic => GeneratorDefaultClientMode::Sync,
            Self::Typescript => GeneratorDefaultClientMode::Async,
            Self::RubySorbet => GeneratorDefaultClientMode::Sync,
        }
    }
}

#[derive(Debug, Clone, strum::Display, strum::EnumString, strum::VariantNames, PartialEq, Eq)]
pub enum GeneratorDefaultClientMode {
    #[strum(serialize = "sync")]
    Sync,
    #[strum(serialize = "async")]
    Async,
}

#[derive(derive_builder::Builder, Debug, Clone)]
pub struct Generator {
    pub name: String,
    pub baml_src: PathBuf,
    pub output_type: GeneratorOutputType,
    default_client_mode: Option<GeneratorDefaultClientMode>,
    output_dir: PathBuf,
    pub version: String,

    pub span: crate::ast::Span,
}

impl Generator {
    pub fn as_baml(&self) -> String {
        format!(
            r#"generator {} {{
    output_type "{}"
    output_dir "{}"
    version "{}"
}}"#,
            self.name,
            self.output_type.to_string(),
            self.output_dir.display(),
            self.version,
        )
    }

    pub fn default_client_mode(&self) -> GeneratorDefaultClientMode {
        self.default_client_mode
            .clone()
            .unwrap_or_else(|| self.output_type.default_client_mode())
    }

    /// Used to new generators when they are created
    pub fn recommended_default_client_mode(&self) -> GeneratorDefaultClientMode {
        self.default_client_mode
            .clone()
            .unwrap_or_else(|| self.output_type.recommended_default_client_mode())
    }

    pub fn output_dir(&self) -> PathBuf {
        self.output_dir.join("baml_client")
    }
}
