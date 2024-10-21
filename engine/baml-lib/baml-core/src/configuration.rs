use crate::PreviewFeature;
pub use baml_types::{GeneratorDefaultClientMode, GeneratorOutputType};
use derive_builder::Builder;
use enumflags2::BitFlags;
use std::path::PathBuf;

#[derive(Debug)]
pub struct Configuration {
    pub generators: Vec<Generator>,
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

#[derive(Debug)]
pub enum Generator {
    Codegen(CodegenGenerator),
    BoundaryCloud(CloudProject),
}

// TODO: we should figure out how to model generator fields using serde, since
// the generator blocks are essentially a serde_json parse
// problem is that serde_json has atrocious error messages and we need to provide
// good error messages to the user
#[derive(Builder, Debug, Clone)]
pub struct CodegenGenerator {
    pub name: String,
    pub baml_src: PathBuf,
    pub output_type: GeneratorOutputType,
    default_client_mode: Option<GeneratorDefaultClientMode>,
    pub on_generate: Vec<String>,
    output_dir: PathBuf,
    pub version: String,

    pub span: crate::ast::Span,
}

impl CodegenGenerator {
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

#[derive(Builder, Debug, Clone)]
pub struct CloudProject {
    pub name: String,
    pub baml_src: PathBuf,
    pub project_id: String,
    pub version: String,

    pub span: crate::ast::Span,
}
