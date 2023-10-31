#![deny(unsafe_code, rust_2018_idioms, missing_docs)]
#![allow(clippy::derive_partial_eq_without_eq)]

//! See the docs on [ParserDatabase](./struct.ParserDatabase.html).
//!
//! ## Scope
//!
//! The ParserDatabase is tasked with gathering information about the schema. It is _connector
//! agnostic_: it gathers information and performs generic validations, leaving connector-specific
//! validations to later phases in datamodel core.
//!
//! ## Terminology
//!
//! Names:
//!
//! - _name_: the item name in the schema for datasources, generators, models, model fields,
//!   composite types, composite type fields, enums and enum variants. The `name:` argument for
//!   unique constraints, primary keys and relations.
//! - _mapped name_: the name inside an `@map()` or `@@map()` attribute of a model, field, enum or
//!   enum value. This is used to determine what the name of the Prisma schema item is in the
//!   database.
//! - _database name_: the name in the database, once both the name of the item and the mapped
//!   name have been taken into account. The logic is always the same: if a mapped name is defined,
//!   then the database name is the mapped name, otherwise it is the name of the item.
//! - _constraint name_: indexes, primary keys and defaults can have a constraint name. It can be
//!   defined with a `map:` argument or be a default, generated name if the `map:` argument is not
//!   provided. These usually require a datamodel connector to be defined.

pub mod walkers;

mod attributes;
mod coerce_expression;
mod context;
mod interner;
mod names;
mod template;
mod types;

use std::collections::HashMap;

pub use coerce_expression::{coerce, coerce_array, coerce_opt};
use either::Either;
use internal_baml_prompt_parser::ast::WithSpan as WithPromptSpan;
pub use internal_baml_schema_ast::ast;
use internal_baml_schema_ast::ast::{SchemaAst, WithName, WithSpan};
pub use types::{PromptVariable, StaticType};

use self::{context::Context, interner::StringId, types::Types};
use internal_baml_diagnostics::{DatamodelError, Diagnostics};
use names::Names;
pub use template::WithSerialize;

/// ParserDatabase is a container for a Schema AST, together with information
/// gathered during schema validation. Each validation step enriches the
/// database with information that can be used to work with the schema, without
/// changing the AST. Instantiating with `ParserDatabase::new()` will perform a
/// number of validations and make sure the schema makes sense, but it cannot
/// fail. In case the schema is invalid, diagnostics will be created and the
/// resolved information will be incomplete.
///
/// Validations are carried out in the following order:
///
/// - The AST is walked a first time to resolve names: to each relevant
///   identifier, we attach an ID that can be used to reference the
///   corresponding item (model, enum, field, ...)
/// - The AST is walked a second time to resolve types. For each field and each
///   type alias, we look at the type identifier and resolve what it refers to.
/// - The AST is walked a third time to validate attributes on models and
///   fields.
/// - Global validations are then performed on the mostly validated schema.
///   Currently only index name collisions.
pub struct ParserDatabase {
    ast: ast::SchemaAst,
    interner: interner::StringInterner,
    names: Names,
    types: Types,
}

impl ParserDatabase {
    /// Create a new, empty ParserDatabase.
    pub fn new() -> Self {
        ParserDatabase {
            ast: ast::SchemaAst { tops: vec![] },
            interner: Default::default(),
            names: Default::default(),
            types: Default::default(),
        }
    }

    /// See the docs on [ParserDatabase](/struct.ParserDatabase.html).
    pub fn add_ast(&mut self, ast: SchemaAst) {
        self.ast.tops.extend(ast.tops);
    }

    /// See the docs on [ParserDatabase](/struct.ParserDatabase.html).
    pub fn validate(&mut self, mut diag: &mut Diagnostics) -> Result<(), Diagnostics> {
        diag.to_result()?;

        let mut ctx = Context::new(
            &self.ast,
            &mut self.interner,
            &mut self.names,
            &mut self.types,
            &mut diag,
        );

        // First pass: resolve names.
        names::resolve_names(&mut ctx);

        // Return early on name resolution errors.
        ctx.diagnostics.to_result()?;

        // Second pass: resolve top-level items and field types.
        types::resolve_types(&mut ctx);

        // Return early on type resolution errors.
        ctx.diagnostics.to_result()?;

        attributes::resolve_attributes(&mut ctx);
        ctx.diagnostics.to_result()?;

        // Third pass: validate attributes.
        self.finalize_prompt_validation(&mut diag)
    }

    fn finalize_prompt_validation(&mut self, diag: &mut Diagnostics) -> Result<(), Diagnostics> {
        let mut vars: HashMap<_, _> = Default::default();

        for variant in self.walk_variants() {
            let mut input_replacers = HashMap::new();
            let mut output_replacers = HashMap::new();
            if let Some(fn_walker) = variant.walk_function() {
                // Now lets validate the prompt is what we expect.
                let prompt_variables = &variant.properties().prompt_replacements;

                prompt_variables.iter().for_each(|f| match f {
                    PromptVariable::Input(variable) => {
                        // Ensure the prompt has an input path that works.
                        match types::post_prompt::process_input(self, fn_walker, variable) {
                            Ok(replacer) => {
                                input_replacers.insert(variable.to_owned(), replacer);
                            }
                            Err(e) => diag.push_error(e),
                        }
                    }
                    PromptVariable::Enum(blk) => {
                        // Ensure the prompt has an enum path that works.
                        match types::post_prompt::process_print_enum(self, fn_walker, &blk) {
                            Ok(result) => {
                                output_replacers.insert(blk.to_owned(), result);
                            }
                            Err(e) => diag.push_error(e),
                        }
                    }
                    PromptVariable::Type(blk) => {
                        // Ensure the prompt has an enum path that works.
                        match types::post_prompt::process_print_type(self, fn_walker, &blk) {
                            Ok(result) => {
                                output_replacers.insert(blk.to_owned(), result);
                            }
                            Err(e) => diag.push_error(e),
                        }
                    }
                });
            } else {
                diag.push_error(DatamodelError::new_type_not_found_error(
                    variant.function_identifier().name(),
                    self.valid_function_names(),
                    variant.function_identifier().span().clone(),
                ));
            }

            // Only in this case update the prompt.
            vars.insert(variant.id, (input_replacers, output_replacers));
        }

        vars.into_iter().for_each(|(k, v)| {
            self.types.variant_properties.get_mut(&k).unwrap().replacers = v;
        });

        diag.to_result()
    }

    /// The parsed AST.
    pub fn ast(&self) -> &ast::SchemaAst {
        &self.ast
    }
    /// The total number of enums in the schema. This is O(1).
    pub fn enums_count(&self) -> usize {
        self.types.enum_attributes.len()
    }
}

impl std::fmt::Debug for ParserDatabase {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("ParserDatabase { ... }")
    }
}

impl std::ops::Index<StringId> for ParserDatabase {
    type Output = str;

    fn index(&self, index: StringId) -> &Self::Output {
        self.interner.get(index).unwrap()
    }
}
