#![doc = include_str!("../README.md")]
#![deny(rust_2018_idioms, unsafe_code, missing_docs)]

use std::path::PathBuf;

pub use internal_baml_core::{
    self,
    internal_baml_diagnostics::{self, Diagnostics, SourceFile},
    internal_baml_parser_database::{self},
    internal_baml_schema_ast, Configuration, StringFromEnvVar, ValidatedSchema,
};

/// Parses and validate a schema, but skip analyzing everything except datasource and generator
/// blocks.
pub fn parse_configuration(
    root_path: &PathBuf,
    path: impl Into<PathBuf>,
    schema: &str,
) -> Result<Configuration, Diagnostics> {
    let source = SourceFile::from((path.into(), schema));
    internal_baml_core::parse_configuration(root_path, &source)
}

/// Parse and analyze a Prisma schema.
pub fn parse_schema(
    root_path: &PathBuf,
    files: impl Into<Vec<SourceFile>>,
) -> Result<ValidatedSchema, Diagnostics> {
    let mut schema = validate(root_path, files.into());
    schema.diagnostics.to_result()?;
    Ok(schema)
}

/// The most general API for dealing with Prisma schemas. It accumulates what analysis and
/// validation information it can, and returns it along with any error and warning diagnostics.
pub fn validate(root_path: &PathBuf, files: Vec<SourceFile>) -> ValidatedSchema {
    internal_baml_core::validate(root_path, files)
}
