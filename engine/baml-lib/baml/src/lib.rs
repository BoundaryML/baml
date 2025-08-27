#![deny(unsafe_code)]

use std::path::{Path, PathBuf};

pub use internal_baml_core::{
    self, internal_baml_ast,
    internal_baml_diagnostics::{self, Diagnostics, SourceFile},
    internal_baml_parser_database::{self},
    Configuration, FeatureFlags, ValidatedSchema,
};

/// Parses and validate a schema, but skip analyzing everything except datasource and generator
/// blocks.
pub fn parse_configuration(
    root_path: &Path,
    path: impl Into<PathBuf>,
    schema: &str,
) -> Result<(Configuration, Diagnostics), Diagnostics> {
    let source = SourceFile::from((path.into(), schema));
    internal_baml_core::validate_single_file(root_path, &source)
}

/// Parse and analyze a Prisma schema.
pub fn parse_and_validate_schema(
    root_path: &Path,
    files: impl Into<Vec<SourceFile>>,
    feature_flags: FeatureFlags,
) -> Result<ValidatedSchema, Diagnostics> {
    let mut schema = validate(root_path, files.into(), feature_flags);
    schema.diagnostics.to_result()?;
    Ok(schema)
}

/// The most general API for dealing with Prisma schemas. It accumulates what analysis and
/// validation information it can, and returns it along with any error and warning diagnostics.
pub fn validate(
    root_path: &Path,
    files: Vec<SourceFile>,
    feature_flags: FeatureFlags,
) -> ValidatedSchema {
    internal_baml_core::validate(root_path, files, feature_flags)
}
