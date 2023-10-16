#![doc = include_str!("../README.md")]
#![deny(rust_2018_idioms, unsafe_code)]
#![allow(clippy::derive_partial_eq_without_eq)]

pub use internal_baml_diagnostics;
pub use internal_baml_parser_database::{self, is_reserved_type_name};
pub use internal_baml_schema_ast::{self, ast, SourceFile};

use internal_baml_diagnostics::Diagnostics;

mod common;
mod configuration;
mod validate;

pub use crate::{
    common::{PreviewFeature, PreviewFeatures, ALL_PREVIEW_FEATURES},
    configuration::{Configuration, StringFromEnvVar},
};

pub struct ValidatedSchema {
    // pub configuration: Configuration,
    pub db: internal_baml_parser_database::ParserDatabase,
    pub diagnostics: Diagnostics,
}

impl std::fmt::Debug for ValidatedSchema {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("<Prisma schema>")
    }
}

/// The most general API for dealing with Prisma schemas. It accumulates what analysis and
/// validation information it can, and returns it along with any error and warning diagnostics.
pub fn validate(file: SourceFile) -> ValidatedSchema {
    let mut diagnostics = Diagnostics::new();
    let db = internal_baml_parser_database::ParserDatabase::new(file, &mut diagnostics);
    let configuration = validate_configuration(db.ast(), &mut diagnostics);
    let out = validate::validate(db, configuration.preview_features(), diagnostics);

    ValidatedSchema {
        diagnostics: out.diagnostics,
        // configuration,
        db: out.db,
    }
}

/// Loads all configuration blocks from a datamodel using the built-in source definitions.
pub fn parse_configuration(schema: &str) -> Result<Configuration, Diagnostics> {
    let mut diagnostics = Diagnostics::default();
    let ast = internal_baml_schema_ast::parse_schema(schema, &mut diagnostics);
    let out = validate_configuration(&ast, &mut diagnostics);
    diagnostics.to_result().map(|_| out)
}

fn validate_configuration(
    schema_ast: &ast::SchemaAst,
    diagnostics: &mut Diagnostics,
) -> Configuration {
    Configuration {
        generators: Vec::new(),
        warnings: diagnostics.warnings().to_owned(),
    }
}
