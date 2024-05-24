use std::path::PathBuf;

use anyhow::Result;
use indexmap::IndexMap;
use internal_baml_core::{configuration::GeneratorOutputType, ir::repr::IntermediateRepr};

mod dir_writer;
mod python;
mod ruby;
mod typescript;

pub struct GeneratorArgs {
    /// Output directory for the generated client, relative to baml_src
    output_dir_relative_to_baml_src: PathBuf,

    /// Path to the BAML source directory
    baml_src_dir: PathBuf,
}

impl GeneratorArgs {
    pub fn new(output_dir: impl Into<PathBuf>, baml_src_dir: impl Into<PathBuf>) -> Self {
        Self {
            output_dir_relative_to_baml_src: output_dir.into(),
            baml_src_dir: baml_src_dir.into(),
        }
    }

    pub fn output_dir(&self) -> PathBuf {
        use sugar_path::SugarPath;
        self.baml_src_dir
            .join(&self.output_dir_relative_to_baml_src)
            .normalize()
    }

    pub fn baml_src_relative_to_output_dir(&self) -> Result<PathBuf> {
        let res = pathdiff::diff_paths(&self.baml_src_dir, &self.output_dir()).ok_or_else(|| {
            anyhow::anyhow!(
                "Failed to compute baml_src ({}) relative to output_dir ({})",
                self.baml_src_dir.display(),
                self.output_dir().display()
            )
        });

        log::info!(
            "baml_src relative to output_dir\nbaml_src: {}\noutput_dir: {}\nrel_baml_src: {:#?}\n",
            self.baml_src_dir.display(),
            self.output_dir().display(),
            res
        );

        res
    }
}

pub struct GenerateOutput {
    pub client_type: GeneratorOutputType,
    pub output_dir: PathBuf,
    pub files: IndexMap<PathBuf, String>,
}

pub trait GenerateClient {
    fn generate_client(&self, ir: &IntermediateRepr, gen: &GeneratorArgs)
        -> Result<GenerateOutput>;
}

impl GenerateClient for GeneratorOutputType {
    fn generate_client(
        &self,
        ir: &IntermediateRepr,
        gen: &GeneratorArgs,
    ) -> Result<GenerateOutput> {
        let files = match self {
            GeneratorOutputType::Ruby => ruby::generate(ir, gen),
            GeneratorOutputType::PythonPydantic => python::generate(ir, gen),
            GeneratorOutputType::Typescript => typescript::generate(ir, gen),
        }?;

        Ok(GenerateOutput {
            client_type: self.clone(),
            output_dir: gen.output_dir(),
            files,
        })
    }
}
