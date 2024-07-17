use anyhow::Result;
use colored::*;
use indexmap::IndexMap;
use internal_baml_core::{configuration::GeneratorOutputType, ir::repr::IntermediateRepr};
use semver::Version;
use std::{collections::BTreeMap, path::PathBuf};
use version_check::{check_version, GeneratorType, VersionCheckMode};

mod dir_writer;
mod python;
mod ruby;
mod typescript;
pub mod version_check;

pub struct GeneratorArgs {
    /// Output directory for the generated client, relative to baml_src
    output_dir_relative_to_baml_src: PathBuf,

    /// Path to the BAML source directory
    baml_src_dir: PathBuf,

    input_file_map: BTreeMap<PathBuf, String>,

    version: String,
    no_version_check: bool,
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
        output_dir_relative_to_baml_src: impl Into<PathBuf>,
        baml_src_dir: impl Into<PathBuf>,
        input_files: impl IntoIterator<Item = (&'i PathBuf, &'i String)>,
        version: String,
        no_version_check: bool,
    ) -> Result<Self> {
        let baml_src = baml_src_dir.into();
        let input_file_map: BTreeMap<PathBuf, String> = input_files
            .into_iter()
            .map(|(k, v)| Ok((relative_path_to_baml_src(k, &baml_src)?, v.clone())))
            .collect::<Result<_>>()?;

        Ok(Self {
            output_dir_relative_to_baml_src: output_dir_relative_to_baml_src.into(),
            baml_src_dir: baml_src.clone(),
            // for the key, whhich is the name, just get the filename
            input_file_map,
            version,
            no_version_check,
        })
    }

    pub fn file_map(&self) -> Result<Vec<(String, String)>> {
        self.input_file_map
            .iter()
            .map(|(k, v)| {
                Ok((
                    serde_json::to_string(&k.display().to_string()).map_err(|e| {
                        anyhow::Error::from(e)
                            .context(format!("Failed to serialize key {:#}", k.display()))
                    })?,
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

pub fn versions_equal_ignoring_patch(v1: &str, v2: &str) -> bool {
    let version1 = Version::parse(v1).unwrap();
    let version2 = Version::parse(v2).unwrap();

    version1.major == version2.major && version1.minor == version2.minor
}

// Assume VSCode is the only one that uses WASM, and it does call this method but at a different time.
#[cfg(target_arch = "wasm32")]
fn version_check_with_error(
    runtime_version: &str,
    gen_version: &str,
    generator_type: GeneratorType,
    mode: VersionCheckMode,
    client_type: GeneratorOutputType,
) -> Result<()> {
    Ok(())
}

#[cfg(not(target_arch = "wasm32"))]
fn version_check_with_error(
    runtime_version: &str,
    gen_version: &str,
    generator_type: GeneratorType,
    mode: VersionCheckMode,
    client_type: GeneratorOutputType,
) -> Result<()> {
    let res = check_version(
        runtime_version,
        gen_version,
        generator_type,
        mode,
        client_type,
    );
    match res {
        Some(e) => Err(anyhow::anyhow!("Version mismatch: {}", e.msg)),
        None => Ok(()),
    }
}

impl GenerateClient for GeneratorOutputType {
    fn generate_client(
        &self,
        ir: &IntermediateRepr,
        gen: &GeneratorArgs,
    ) -> Result<GenerateOutput> {
        let runtime_version = env!("CARGO_PKG_VERSION");

        if !gen.no_version_check {
            version_check_with_error(
                runtime_version,
                &gen.version,
                GeneratorType::CLI,
                VersionCheckMode::Strict,
                self.clone(),
            )?;
        }

        let files = match self {
            GeneratorOutputType::RubySorbet => ruby::generate(ir, gen),
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
