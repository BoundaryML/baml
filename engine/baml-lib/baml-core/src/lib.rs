#![deny(rust_2018_idioms, unsafe_code)]
#![allow(clippy::derive_partial_eq_without_eq)]

use std::{
    path::{Path, PathBuf},
    sync::Mutex,
};

use enumflags2::BitFlags;
use internal_baml_ast::ast::{Identifier, WithName};
pub use internal_baml_ast::{self, ast};
pub use internal_baml_diagnostics;
use internal_baml_diagnostics::{DatamodelError, Diagnostics, SourceFile, Span};
use internal_baml_parser_database::TypeWalker;
pub use internal_baml_parser_database::{self};
use ir::repr::WithRepr;
use rayon::prelude::*;

mod common;
pub mod configuration;
pub mod feature_flags;
pub mod ir;
// mod lockfile;
mod validate;

use self::validate::generator_loader;
pub use crate::{
    common::{PreviewFeature, PreviewFeatures, ALL_PREVIEW_FEATURES},
    configuration::Configuration,
    feature_flags::{BamlFeatureFlag, FeatureFlags},
};

pub struct ValidatedSchema {
    pub db: internal_baml_parser_database::ParserDatabase,
    pub diagnostics: Diagnostics,
    pub configuration: Configuration,
}

impl std::fmt::Debug for ValidatedSchema {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("<Prisma schema>")
    }
}

/// The most general API for dealing with BAML source code. It accumulates what analysis and
/// validation information it can, and returns it along with any error and warning diagnostics.
pub fn validate(
    root_path: &Path,
    files: Vec<SourceFile>,
    feature_flags: FeatureFlags,
) -> ValidatedSchema {
    let mut diagnostics = Diagnostics::new(root_path.to_path_buf());
    let mut db = internal_baml_parser_database::ParserDatabase::new();

    {
        let diagnostics = Mutex::new(&mut diagnostics);
        let db = Mutex::new(&mut db);
        files
            .par_iter()
            .for_each(|file| match internal_baml_ast::parse(root_path, file) {
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

    if let Err(d) = db.validate(&mut diagnostics) {
        return ValidatedSchema {
            db,
            diagnostics: d,
            configuration: Configuration::new(),
        };
    }

    let (mut configuration, diag) = validate_config_impl(root_path, db.ast());
    diagnostics.push(diag);

    // Set the feature flags on the configuration
    configuration.feature_flags = feature_flags;

    if diagnostics.has_errors() {
        return ValidatedSchema {
            db,
            diagnostics,
            configuration,
        };
    }

    // actually run the validation pipeline
    validate::validate(&db, &configuration, &mut diagnostics);

    if diagnostics.has_errors() {
        return ValidatedSchema {
            db,
            diagnostics,
            configuration,
        };
    }

    // Some last linker stuff can only happen post validation.
    db.finalize(&mut diagnostics);

    // TODO: #1343 Temporary solution until we implement scoping in the AST.
    validate_test_type_builders(&mut diagnostics, &mut db, &configuration);

    ValidatedSchema {
        db,
        diagnostics,
        configuration,
    }
}

/// TODO: #1343 Temporary solution until we implement scoping in the AST.
///
/// TODO: This is a very ugly hack to implement scoping for type builder blocks
/// in test cases. Type builder blocks support all the type definitions (class,
/// enum, type alias), and all these definitions have access to both the global
/// and local scope but not the scope of other test cases.
///
/// This codebase was not designed with scoping in mind, so there's no simple
/// way of implementing scopes in the AST and IR.
///
/// # Hack Explanation
///
/// For every single type_builder block within a test we are creating a separate
/// instance of [`internal_baml_parser_database::ParserDatabase`] that includes
/// both the global type defs and the local type builder defs in the same AST.
/// That way we can run all the validation logic that we normally execute for
/// the global scope but including the local scope as well, and it doesn't
/// become too complicated to figure out stuff like name resolution, alias
/// resolution, dependencies, etc.
///
/// However, this increases memory usage significantly since we create a copy
/// of the entire AST for every single type builder block. We implemented it
/// this way because we wanted to ship the feature and delay AST refactoring,
/// since it would take much longer to refactor the AST and include scoping than
/// it would take to ship this hack.
fn validate_test_type_builders(
    diagnostics: &mut Diagnostics,
    db: &mut internal_baml_parser_database::ParserDatabase,
    configuration: &Configuration,
) {
    let mut test_case_scoped_dbs = Vec::new();
    for test in db.walk_test_cases() {
        let mut scoped_db = internal_baml_parser_database::ParserDatabase::new();
        scoped_db.add_ast(db.ast().to_owned());

        let Some(type_builder) = test.test_case().type_builder.as_ref() else {
            continue;
        };

        let local_ast = validate_type_builder_entries(diagnostics, db, &type_builder.entries);

        scoped_db.add_ast(local_ast);

        if let Err(d) = scoped_db.validate(diagnostics) {
            diagnostics.push(d);
            continue;
        }
        validate::validate(&scoped_db, configuration, diagnostics);
        if diagnostics.has_errors() {
            continue;
        }
        scoped_db.finalize(diagnostics);

        test_case_scoped_dbs.push((test.id.0, scoped_db));
    }
    for (test_id, scoped_db) in test_case_scoped_dbs.into_iter() {
        db.add_test_case_db(test_id, scoped_db);
    }
}

pub fn run_validation_pipeline_on_db(
    db: &mut internal_baml_parser_database::ParserDatabase,
    diagnostics: &mut Diagnostics,
) {
    let configuration = Configuration::new(); // Use default configuration with no feature flags
    validate::validate(db, &configuration, diagnostics);
    if diagnostics.has_errors() {
        return;
    }
    db.finalize(diagnostics);
}

/// TODO: #1343 Temporary solution until we implement scoping in the AST.
///
/// See [`validate_test_type_builders`] for more information.
pub fn validate_type_builder_entries(
    diagnostics: &mut Diagnostics,
    db: &internal_baml_parser_database::ParserDatabase,
    entries: &[ast::TypeBuilderEntry],
) -> ast::Ast {
    let mut local_ast = ast::Ast::new();
    for type_def in entries {
        local_ast.tops.push(match type_def {
            ast::TypeBuilderEntry::Class(c) => {
                if c.attributes.iter().any(|attr| attr.name.name() == "dynamic") {
                    diagnostics.push_error(DatamodelError::new_validation_error(
                        "The `@@dynamic` attribute is not allowed in type_builder blocks",
                        c.span.to_owned(),
                    ));
                    continue;
                }

                ast::Top::Class(c.to_owned())
            },
            ast::TypeBuilderEntry::Enum(e) => {
                if e.attributes.iter().any(|attr| attr.name.name() == "dynamic") {
                    diagnostics.push_error(DatamodelError::new_validation_error(
                        "The `@@dynamic` attribute is not allowed in type_builder blocks",
                        e.span.to_owned(),
                    ));
                    continue;
                }

                ast::Top::Enum(e.to_owned())
            },
            ast::TypeBuilderEntry::Dynamic(d) => {
                if d.attributes.iter().any(|attr| attr.name.name() == "dynamic") {
                    diagnostics.push_error(DatamodelError::new_validation_error(
                        "Dynamic type definitions cannot contain the `@@dynamic` attribute",
                        d.span.to_owned(),
                    ));
                    continue;
                }

                let mut dyn_type = d.to_owned();

                // TODO: Extemely ugly hack to avoid collisions in the name
                // interner. We use syntax that is not normally allowed by
                // BAML for type names.
                dyn_type.name = Identifier::Local(
                    format!("{}{}", ast::DYNAMIC_TYPE_NAME_PREFIX, dyn_type.name()),
                    dyn_type.span.to_owned(),
                );

                // TODO: Not necessary, the parser also does this now that we've
                // change "dynamic ClassName" to "dynamic class ClassName".
                dyn_type.is_dynamic_type_def = true;

                // Resolve dynamic definition. It either appends to a
                // @@dynamic class or enum.
                match db.find_type_by_str(d.name()) {
                    Some(t) => match t {
                        TypeWalker::Class(cls) => {
                            if !cls.ast_type_block().attributes.iter().any(|attr| attr.name.name() == "dynamic") {
                                diagnostics.push_error(DatamodelError::new_validation_error(
                                    &format!(
                                        "Type '{}' does not contain the `@@dynamic` attribute so it cannot be modified in a type builder block",
                                        cls.name()
                                    ),
                                    dyn_type.span.to_owned(),
                                ));
                                continue;
                            }

                            if matches!(dyn_type.sub_type, ast::SubType::Enum) {
                                diagnostics.push_error(DatamodelError::new_validation_error(
                                    &format!(
                                        "Type '{}' is a class, but the dynamic block is defined as 'dynamic enum'",
                                        cls.name()
                                    ),
                                    dyn_type.span.to_owned(),
                                ));
                                continue;
                            }

                            ast::Top::Class(dyn_type)
                        },
                        TypeWalker::Enum(enm) => {
                            if !enm.ast_type_block().attributes.iter().any(|attr| attr.name.name() == "dynamic") {
                                diagnostics.push_error(DatamodelError::new_validation_error(
                                    &format!(
                                        "Type '{}' does not contain the `@@dynamic` attribute so it cannot be modified in a type builder block",
                                        enm.name()
                                    ),
                                    dyn_type.span.to_owned(),
                                ));
                                continue;
                            }

                            if matches!(dyn_type.sub_type, ast::SubType::Class) {
                                diagnostics.push_error(DatamodelError::new_validation_error(
                                    &format!(
                                        "Type '{}' is an enum, but the dynamic block is defined as 'dynamic class'",
                                        enm.name()
                                    ),
                                    dyn_type.span.to_owned(),
                                ));
                                continue;
                            }

                            ast::Top::Enum(dyn_type)
                        },
                        TypeWalker::TypeAlias(_) => {
                            diagnostics.push_error(DatamodelError::new_validation_error(
                                &format!("The `dynamic` keyword only works on classes and enums, but type '{}' is a type alias", d.name()),
                                d.span.to_owned(),
                            ));
                            continue;
                        },
                    },
                    None => {
                        diagnostics.push_error(DatamodelError::new_validation_error(
                            &format!("Type '{}' not found", d.name()),
                            dyn_type.span.to_owned(),
                        ));
                        continue;
                    }
                }
            }
            ast::TypeBuilderEntry::TypeAlias(assignment) => {
                ast::Top::TypeAlias(assignment.to_owned())
            },
        });
    }
    local_ast
}

/// Loads all configuration blocks from a datamodel using the built-in source definitions.
pub fn validate_single_file(
    root_path: &Path,
    main_schema: &SourceFile,
) -> Result<(Configuration, Diagnostics), Diagnostics> {
    let (ast, mut diagnostics) = internal_baml_ast::parse(root_path, main_schema)?;

    let (out, diag) = validate_config_impl(root_path, &ast);
    diagnostics.push(diag);

    if out.generators.is_empty() {
        diagnostics.push_error(DatamodelError::new_validation_error(
            "No generator specified",
            Span {
                file: main_schema.clone(),
                start: 0,
                end: 0,
            },
        ));
    }

    if diagnostics.has_errors() {
        return Err(diagnostics);
    }

    Ok((out, diagnostics))
}

fn validate_config_impl(
    root_path: &Path,
    ast: &ast::Ast,
    // skip_lock_file_validation: bool,
) -> (Configuration, Diagnostics) {
    let mut diagnostics = Diagnostics::new(root_path.to_path_buf());
    let generators = generator_loader::load_generators_from_ast(ast, &mut diagnostics);

    // let lock_files = generators
    //     .iter()
    //     .filter_map(
    //         |gen| match lockfile::LockFileWrapper::from_generator(&gen) {
    //             Ok(lock_file) => {
    //                 if let Ok(prev) =
    //                     lockfile::LockFileWrapper::from_path(gen.output_dir().join("baml.lock"))
    //                 {
    //                     lock_file.validate(&prev, &mut diagnostics);
    //                 }
    //                 Some((gen.clone(), lock_file))
    //             }
    //             Err(err) => {
    //                 diagnostics.push_error(DatamodelError::new_validation_error(
    //                     &format!("Failed to create lock file: {}", err),
    //                     gen.span.clone(),
    //                 ));
    //                 None
    //             }
    //         },
    //     )
    //     .collect();

    (
        Configuration {
            generators,
            feature_flags: FeatureFlags::new(), // Default empty, will be set by main validate function
        },
        diagnostics,
    )
}
