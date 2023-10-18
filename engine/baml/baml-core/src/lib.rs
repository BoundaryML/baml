#![doc = include_str!("../README.md")]
#![deny(rust_2018_idioms, unsafe_code)]
#![allow(clippy::derive_partial_eq_without_eq)]

pub use internal_baml_diagnostics;
use internal_baml_parser_database::ParserDatabase;
pub use internal_baml_parser_database::{self, is_reserved_type_name};
use internal_baml_schema_ast::ast::SchemaAst;
pub use internal_baml_schema_ast::{self, ast};
use rayon::prelude::*;
use std::sync::Mutex;

use internal_baml_diagnostics::{Diagnostics, SourceFile};

mod common;
mod configuration;
mod validate;

use self::validate::generator_loader;
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
pub fn validate(files: Vec<SourceFile>) -> ValidatedSchema {
    let mut diagnostics = Diagnostics::new();
    let mut db = internal_baml_parser_database::ParserDatabase::new();

    {
        let diagnostics = Mutex::new(&mut diagnostics);
        let db = Mutex::new(&mut db);
        files
            .par_iter()
            .for_each(|file| match internal_baml_schema_ast::parse_schema(file) {
                Ok((ast, err)) => {
                    let mut diagnostics = diagnostics.lock().unwrap();
                    let mut db = db.lock().unwrap();
                    diagnostics.push(err);
                    db.add_ast(ast);
                }
                Err(err) => {
                    let mut diagnostics = diagnostics.lock().unwrap();
                    diagnostics.push(err);
                }
            });
    }

    if diagnostics.has_errors() {
        return ValidatedSchema { db, diagnostics };
    }

    let (configuration, diag) = validate_configuration(db.ast());
    diagnostics.push(diag);

    if diagnostics.has_errors() {
        return ValidatedSchema { db, diagnostics };
    }

    validate::validate(&db, configuration.preview_features(), &mut diagnostics);

    ValidatedSchema { db, diagnostics }
}

/// Loads all configuration blocks from a datamodel using the built-in source definitions.
pub fn parse_configuration(main_schema: &SourceFile) -> Result<Configuration, Diagnostics> {
    let (ast, mut diagnostics) = internal_baml_schema_ast::parse_schema(main_schema)?;

    let (out, diag) = validate_configuration(&ast);
    diagnostics.push(diag);
    diagnostics.to_result().map(|_| out)
}

fn validate_configuration(schema_ast: &ast::SchemaAst) -> (Configuration, Diagnostics) {
    let mut diagnostics = Diagnostics::new();
    let generators = generator_loader::load_generators_from_ast(schema_ast, &mut diagnostics);

    (Configuration { generators }, diagnostics)
}
