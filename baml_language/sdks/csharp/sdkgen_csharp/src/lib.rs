//! C# SDK generation infrastructure.
//!
//! Identifier allocation, file routing, and output replacement are completed
//! before rendering so generated source cannot depend on discovery order or a
//! partially written output tree.

pub mod names;
pub mod pipeline;
pub mod routing;

mod model;
mod normalize;
mod semantic;
mod transaction;

use std::{fmt, fs, io, path::PathBuf};

use baml_codegen_types::SymbolPool;
pub use semantic::CSharpGenerationError;
pub use transaction::{GenerationManifest, TransactionError};

/// Complete input for one C# generation transaction.
pub struct CSharpGenerateRequest<'a> {
    pub symbols: &'a SymbolPool,
    pub program_bytes: &'a [u8],
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
    Transaction(TransactionError),
    InvalidOutputDirectory(PathBuf),
    Io {
        operation: &'static str,
        path: PathBuf,
        source: io::Error,
    },
}

impl fmt::Display for GenerateIntoError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Generation(error) => error.fmt(formatter),
            Self::Transaction(error) => error.fmt(formatter),
            Self::InvalidOutputDirectory(path) => write!(
                formatter,
                "C# generated output must be an absolute directory path: `{}`",
                path.display()
            ),
            Self::Io {
                operation,
                path,
                source,
            } => write!(formatter, "{operation} `{}`: {source}", path.display()),
        }
    }
}

impl std::error::Error for GenerateIntoError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Generation(error) => Some(error),
            Self::Transaction(error) => Some(error),
            Self::Io { source, .. } => Some(source),
            Self::InvalidOutputDirectory(_) => None,
        }
    }
}

impl From<CSharpGenerationError> for GenerateIntoError {
    fn from(error: CSharpGenerationError) -> Self {
        Self::Generation(error)
    }
}

impl From<TransactionError> for GenerateIntoError {
    fn from(error: TransactionError) -> Self {
        Self::Transaction(error)
    }
}

/// Generate, validate, stage, and atomically install a complete C# source tree.
pub fn generate_into(
    request: CSharpGenerateRequest<'_>,
) -> Result<GenerationReport, GenerateIntoError> {
    if !request.output_directory.is_absolute() {
        return Err(GenerateIntoError::InvalidOutputDirectory(
            request.output_directory,
        ));
    }
    let parent = request.output_directory.parent().ok_or_else(|| {
        GenerateIntoError::InvalidOutputDirectory(request.output_directory.clone())
    })?;
    let directory = request
        .output_directory
        .file_name()
        .map(PathBuf::from)
        .ok_or_else(|| {
            GenerateIntoError::InvalidOutputDirectory(request.output_directory.clone())
        })?;
    fs::create_dir_all(parent).map_err(|source| GenerateIntoError::Io {
        operation: "create C# generated output parent",
        path: parent.to_path_buf(),
        source,
    })?;

    let model = model::CodegenModel::from_symbol_pool(request.symbols);
    let runtime_identities =
        model::RuntimeCallableIdentities::from_program_bytes(request.program_bytes)
            .map_err(CSharpGenerationError::Unsupported)?;
    let tree = semantic::generate_program_with_runtime_identities(
        &model,
        &runtime_identities,
        request.program_bytes,
        request.cli_version,
        request.required_bridge_version,
        request.program_identity,
    )?;
    let manifest = transaction::commit_generated_tree(parent, &directory, &tree)?;
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
