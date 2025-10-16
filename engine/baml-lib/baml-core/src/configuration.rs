use std::path::PathBuf;

pub use baml_types::{GeneratorDefaultClientMode, GeneratorOutputType};
use bstd::ProjectFqn;
use derive_builder::Builder;
use enumflags2::BitFlags;

use crate::{feature_flags::FeatureFlags, PreviewFeature};

#[derive(Debug)]
pub struct Configuration {
    pub generators: Vec<Generator>,
    pub feature_flags: FeatureFlags,
}

impl Default for Configuration {
    fn default() -> Self {
        Self::new()
    }
}

impl Configuration {
    pub fn new() -> Self {
        Self {
            generators: vec![],
            feature_flags: FeatureFlags::new(),
        }
    }

    pub fn new_with_feature_flags(feature_flags: FeatureFlags) -> Self {
        Self {
            generators: vec![],
            feature_flags,
        }
    }

    pub fn preview_features(&self) -> BitFlags<PreviewFeature> {
        self.generators
            .iter()
            .fold(BitFlags::empty(), |acc, _generator| acc)
    }

    pub fn feature_flags(&self) -> &FeatureFlags {
        &self.feature_flags
    }
}

#[derive(Debug)]
pub enum Generator {
    Codegen(CodegenGenerator),
    BoundaryCloud(CloudProject),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModuleFormat {
    Cjs,
    Esm,
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
    pub client_package_name: Option<String>,
    // For TS generators, we can choose between CJS and ESM module formats
    pub module_format: Option<ModuleFormat>,
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
            self.output_type,
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

    /// Fully-qualified project ID, i.e. @boundaryml/baml
    pub project_fqn: ProjectFqn,

    pub version: String,

    pub span: crate::ast::Span,
}
