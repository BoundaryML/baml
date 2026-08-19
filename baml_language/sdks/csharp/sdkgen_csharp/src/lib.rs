//! C# SDK generation infrastructure.
//!
//! Identifier allocation, file routing, and C# validation are completed before
//! the shared SDK output writer installs the generated tree.

pub mod names;
pub mod pipeline;
pub mod routing;

mod model;
mod normalize;
mod output;
mod semantic;

use std::{fmt, path::PathBuf};

use baml_codegen_types::{
    OutputWriterError, OutputWriterOptions, SymbolPool, write_generated_output_with_options,
};
pub use output::{GenerationManifest, OutputValidationError};
pub use semantic::CSharpGenerationError;

/// Complete input for one C# generation transaction.
pub struct CSharpGenerateRequest<'a> {
    pub symbols: &'a SymbolPool,
    pub program_bytes: &'a [u8],
    pub embedded_baml_toml: &'a str,
    pub cli_version: &'a str,
    pub required_bridge_version: &'a str,
    pub program_identity: &'a str,
    pub output_directory: PathBuf,
}

/// Installed output inventory from a completed C# generation transaction.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GenerationReport {
    pub output_directory: PathBuf,
    pub manifest: GenerationManifest,
    pub written_files: Vec<PathBuf>,
}

#[derive(Debug)]
pub enum GenerateIntoError {
    Generation(CSharpGenerationError),
    Validation(OutputValidationError),
    OutputWriter(OutputWriterError),
    InvalidOutputDirectory(PathBuf),
}

impl fmt::Display for GenerateIntoError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Generation(error) => error.fmt(formatter),
            Self::Validation(error) => error.fmt(formatter),
            Self::OutputWriter(error) => error.fmt(formatter),
            Self::InvalidOutputDirectory(path) => write!(
                formatter,
                "C# generated output must be an absolute directory path: `{}`",
                path.display()
            ),
        }
    }
}

impl std::error::Error for GenerateIntoError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Generation(error) => Some(error),
            Self::Validation(error) => Some(error),
            Self::OutputWriter(error) => Some(error),
            Self::InvalidOutputDirectory(_) => None,
        }
    }
}

impl From<CSharpGenerationError> for GenerateIntoError {
    fn from(error: CSharpGenerationError) -> Self {
        Self::Generation(error)
    }
}

impl From<OutputValidationError> for GenerateIntoError {
    fn from(error: OutputValidationError) -> Self {
        Self::Validation(error)
    }
}

impl From<OutputWriterError> for GenerateIntoError {
    fn from(error: OutputWriterError) -> Self {
        Self::OutputWriter(error)
    }
}

/// Generate and validate C# source, then install it through the shared writer.
pub fn generate_into(
    request: CSharpGenerateRequest<'_>,
) -> Result<GenerationReport, GenerateIntoError> {
    generate_into_with_options(request, OutputWriterOptions::default())
}

/// Generate and validate C# source, then install it with caller-selected writer policy.
pub fn generate_into_with_options(
    request: CSharpGenerateRequest<'_>,
    output_options: OutputWriterOptions,
) -> Result<GenerationReport, GenerateIntoError> {
    if !request.output_directory.is_absolute() {
        return Err(GenerateIntoError::InvalidOutputDirectory(
            request.output_directory,
        ));
    }
    let model = model::CodegenModel::from_symbol_pool(request.symbols);
    let runtime_identities =
        model::RuntimeCallableIdentities::from_program_bytes(request.program_bytes)
            .map_err(CSharpGenerationError::Unsupported)?;
    let tree = semantic::generate_program_with_runtime_identities(
        &model,
        &runtime_identities,
        request.program_bytes,
        request.embedded_baml_toml,
        request.cli_version,
        request.required_bridge_version,
        request.program_identity,
    )?;
    let (manifest, files) = output::validate_and_collect(&tree)?;
    write_generated_output_with_options(&request.output_directory, files, output_options)?;
    let written_files = manifest
        .files
        .iter()
        .map(|entry| request.output_directory.join(&entry.relative_path))
        .collect();
    Ok(GenerationReport {
        output_directory: request.output_directory,
        manifest,
        written_files,
    })
}
