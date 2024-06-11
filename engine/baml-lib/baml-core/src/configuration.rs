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

#[derive(derive_builder::Builder, Debug, Clone)]
pub struct Generator {
    pub name: String,
    pub baml_src: PathBuf,
    pub output_type: GeneratorOutputType,
    output_dir: PathBuf,

    pub(crate) span: crate::ast::Span,
}

impl Generator {
    pub fn as_baml(&self) -> String {
        format!(
            r#"generator {} {{
    output_type "{}"
    output_dir "{}"
}}"#,
            self.name,
            self.output_type.to_string(),
            self.output_dir.display(),
        )
    }

    pub fn output_dir(&self) -> PathBuf {
        self.output_dir.join("baml_client")
    }
}
