#![doc = include_str!("../README.md")]
#![deny(rust_2018_idioms, unsafe_code, missing_docs)]

pub use internal_baml_core::{
    self,
    internal_baml_diagnostics::{self, Diagnostics},
    internal_baml_parser_database::{self, SourceFile},
    internal_baml_schema_ast, Configuration, StringFromEnvVar, ValidatedSchema,
};

/// Parses and validate a schema, but skip analyzing everything except datasource and generator
/// blocks.
pub fn parse_configuration(schema: &str) -> Result<Configuration, Diagnostics> {
    internal_baml_core::parse_configuration(schema)
}

/// Parse and analyze a Prisma schema.
pub fn parse_schema(file: impl Into<SourceFile>) -> Result<ValidatedSchema, String> {
    let mut schema = validate(file.into());
    schema
        .diagnostics
        .to_result()
        .map_err(|err| err.to_pretty_string("schema.prisma", schema.db.source()))?;
    Ok(schema)
}

/// The most general API for dealing with Prisma schemas. It accumulates what analysis and
/// validation information it can, and returns it along with any error and warning diagnostics.
pub fn validate(file: SourceFile) -> ValidatedSchema {
    internal_baml_core::validate(file)
}
