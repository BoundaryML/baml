use std::{collections::BTreeMap, path::PathBuf};

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

    input_file_map: BTreeMap<PathBuf, String>,
}

fn relative_path_to_baml_src(path: &PathBuf, baml_src: &PathBuf) -> Result<PathBuf> {
    pathdiff::diff_paths(path, baml_src).ok_or_else(|| {
        anyhow::anyhow!(
            "Failed to compute relative path from {} to {}",
            path.display(),
            baml_src.display()
        )
    })
}

impl GeneratorArgs {
    pub fn new<'i>(
        output_dir: impl Into<PathBuf>,
        baml_src_dir: impl Into<PathBuf>,
        input_files: impl IntoIterator<Item = (&'i PathBuf, &'i String)>,
    ) -> Result<Self> {
        let baml_src = baml_src_dir.into();
        let input_file_map: BTreeMap<PathBuf, String> = input_files
            .into_iter()
            .map(|(k, v)| Ok((relative_path_to_baml_src(k, &baml_src)?, v.clone())))
            .collect::<Result<_>>()?;

        Ok(Self {
            output_dir_relative_to_baml_src: output_dir.into(),
            baml_src_dir: baml_src.clone(),
            // for the key, whhich is the name, just get the filename
            input_file_map,
        })
    }

    pub fn file_map(&self) -> Result<Vec<(String, String)>> {
        self.input_file_map
            .iter()
            .map(|(k, v)| {
                Ok((
                    k.display().to_string(),
                    serde_json::to_string(v).map_err(|e| {
                        anyhow::Error::from(e)
                            .context(format!("Failed to serialize contents of {:#}", k.display()))
                    })?,
                ))
            })
            .collect()
    }

    pub fn output_dir(&self) -> PathBuf {
        use sugar_path::SugarPath;
        self.baml_src_dir
            .join(&self.output_dir_relative_to_baml_src)
            .normalize()
    }

    /// Returns baml_src relative to the output_dir.
    ///
    /// We need this to be able to codegen a singleton client, so that our generated code can build
    /// baml_src relative to the path of the file in which the singleton is defined. In Python this is
    /// os.path.dirname(__file__) for globals.py; in TS this is __dirname for globals.ts.
    pub fn baml_src_relative_to_output_dir(&self) -> Result<PathBuf> {
        pathdiff::diff_paths(&self.baml_src_dir, &self.output_dir()).ok_or_else(|| {
            anyhow::anyhow!(
                "Failed to compute baml_src ({}) relative to output_dir ({})",
                self.baml_src_dir.display(),
                self.output_dir().display()
            )
        })
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
