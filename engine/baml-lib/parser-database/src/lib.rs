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
mod printer;
mod types;

use std::collections::{HashMap, HashSet};

pub use coerce_expression::{coerce, coerce_array, coerce_opt};
use either::Either;
pub use internal_baml_schema_ast::ast;
use internal_baml_schema_ast::ast::{SchemaAst, WithIdentifier, WithName, WithSpan};
pub use printer::WithStaticRenames;
pub use types::{
    ContantDelayStrategy, DynamicStringAttributes, ExponentialBackoffStrategy, PrinterType,
    PromptAst, PromptVariable, RetryPolicy, RetryPolicyStrategy, StaticStringAttributes,
    StaticType, ToStringAttributes,
};

use self::{context::Context, interner::StringId, types::Types};
use internal_baml_diagnostics::{DatamodelError, DatamodelWarning, Diagnostics};
use names::Names;
pub use printer::WithSerialize;

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

impl Default for ParserDatabase {
    fn default() -> Self {
        Self::new()
    }
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
    pub fn validate(&mut self, diag: &mut Diagnostics) -> Result<(), Diagnostics> {
        diag.to_result()?;

        let mut ctx = Context::new(
            &self.ast,
            &mut self.interner,
            &mut self.names,
            &mut self.types,
            diag,
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
        ctx.diagnostics.to_result()
    }

    /// Updates the prompt
    pub fn finalize(&mut self, diag: &mut Diagnostics) {
        self.link_functions(diag);
        self.finalize_dependencies(diag);
        self.finalize_prompt_validation(diag);
    }

    fn link_functions(&mut self, _diag: &mut Diagnostics) {
        let default_impl = self
            .walk_functions()
            .filter_map(|f| {
                if f.metadata().default_impl.is_none() {
                    return f.walk_variants().find_map(|v| {
                        Some((f.id, v.name().to_string(), f.identifier().span().clone()))
                    });
                }
                None
            })
            .collect::<Vec<_>>();

        default_impl.iter().for_each(|(fid, impl_name, span)| {
            self.types.function.get_mut(fid).unwrap().default_impl =
                Some((impl_name.clone(), span.clone()))
        })
    }

    fn finalize_dependencies(&mut self, diag: &mut Diagnostics) {
        let mut deps = self
            .types
            .class_dependencies
            .iter()
            .map(|f| {
                (
                    *f.0,
                    f.1.iter()
                        .fold((0, 0, 0), |prev, i| match self.find_type_by_str(i) {
                            Some(Either::Left(_)) => (prev.0 + 1, prev.1 + 1, prev.2),
                            Some(Either::Right(_)) => (prev.0 + 1, prev.1, prev.2 + 1),
                            _ => prev,
                        }),
                )
            })
            .collect::<Vec<_>>();

        // Can only process deps which have 0 class dependencies.
        let mut max_loops = 100;
        while !deps.is_empty() && max_loops > 0 {
            max_loops -= 1;
            // Remove all the ones which have 0 class dependencies.
            let removed = deps
                .iter()
                .filter(|(_, v)| v.1 == 0)
                .map(|(k, _)| *k)
                .collect::<Vec<_>>();
            deps.retain(|(_, v)| v.1 > 0);
            for cls in removed {
                let child_deps = self
                    .types
                    .class_dependencies
                    .get(&cls)
                    // These must exist by definition so safe to unwrap.
                    .unwrap()
                    .iter()
                    .filter_map(|f| match self.find_type_by_str(f) {
                        Some(Either::Left(walker)) => {
                            Some(walker.dependencies().iter().cloned().collect::<Vec<_>>())
                        }
                        Some(Either::Right(walker)) => Some(vec![walker.name().to_string()]),
                        _ => panic!("Unknown class `{}`", f),
                    })
                    .flatten()
                    .collect::<HashSet<_>>();
                let name = self.ast[cls].name();
                deps.iter_mut()
                    .filter(|(k, _)| self.types.class_dependencies[k].contains(name))
                    .for_each(|(_, v)| {
                        v.1 -= 1;
                    });

                // Get the dependencies of all my dependencies.
                self.types
                    .class_dependencies
                    .get_mut(&cls)
                    .unwrap()
                    .extend(child_deps);
            }
        }

        if max_loops == 0 && !deps.is_empty() {
            let circular_deps = deps
                .iter()
                .map(|(k, _)| self.ast[*k].name())
                .collect::<Vec<_>>()
                .join(" -> ");

            deps.iter().for_each(|(k, _)| {
                diag.push_error(DatamodelError::new_validation_error(
                    &format!(
                        "Circular dependency detected for class `{}`.\n{}",
                        self.ast[*k].name(),
                        circular_deps
                    ),
                    self.ast[*k].identifier().span().clone(),
                ));
            });
        }

        // Additionally ensure the same thing for functions, but since we've already handled classes,
        // this should be trivial.
        let extends = self
            .types
            .function
            .iter()
            .map(|(&k, func)| {
                let (input, output) = &func.dependencies;
                let input_deps = input
                    .iter()
                    .filter_map(|f| match self.find_type_by_str(f) {
                        Some(Either::Left(walker)) => Some(walker.dependencies().iter().cloned()),
                        Some(Either::Right(_)) => None,
                        _ => panic!("Unknown class `{}`", f),
                    })
                    .flatten()
                    .collect::<HashSet<_>>();

                let output_deps = output
                    .iter()
                    .filter_map(|f| match self.find_type_by_str(f) {
                        Some(Either::Left(walker)) => Some(walker.dependencies().iter().cloned()),
                        Some(Either::Right(_)) => None,
                        _ => panic!("Unknown class `{}`", f),
                    })
                    .flatten()
                    .collect::<HashSet<_>>();

                (k, (input_deps, output_deps))
            })
            .collect::<Vec<_>>();

        for (id, (input, output)) in extends {
            let val = self.types.function.get_mut(&id).unwrap();
            val.dependencies.0.extend(input);
            val.dependencies.1.extend(output);
        }
    }

    fn finalize_prompt_validation(&mut self, diag: &mut Diagnostics) {
        let mut vars: HashMap<_, _> = Default::default();

        self.walk_variants().for_each(|variant| {
            let mut input_replacers = HashMap::new();
            let mut output_replacers = HashMap::new();
            let mut chat_replacers = vec![];
            if let Some(fn_walker) = variant.walk_function() {
                // Now lets validate the prompt is what we expect.
                let prompt_variables = &variant.properties().prompt_replacements;

                let num_errors = prompt_variables.iter().fold(0, |count, f| match f {
                    PromptVariable::Input(variable) => {
                        // Ensure the prompt has an input path that works.
                        match types::post_prompt::process_input(self, fn_walker, variable) {
                            Ok(replacer) => {
                                input_replacers.insert(variable.to_owned(), replacer);
                                count
                            }
                            Err(e) => {
                                diag.push_error(e);
                                count + 1
                            }
                        }
                    }
                    PromptVariable::Enum(blk) => {
                        // Ensure the prompt has an enum path that works.
                        match types::post_prompt::process_print_enum(
                            self, variant, fn_walker, blk, diag,
                        ) {
                            Ok(result) => {
                                output_replacers.insert(blk.to_owned(), result);
                                count
                            }
                            Err(e) => {
                                diag.push_error(e);
                                count + 1
                            }
                        }
                    }
                    PromptVariable::Type(blk) => {
                        // Ensure the prompt has an enum path that works.
                        match types::post_prompt::process_print_type(self, variant, fn_walker, blk)
                        {
                            Ok(result) => {
                                output_replacers.insert(blk.to_owned(), result);
                                count
                            }
                            Err(e) => {
                                diag.push_error(e);
                                count + 1
                            }
                        }
                    }
                    PromptVariable::Chat(c) => {
                        chat_replacers.push(c.clone());
                        count
                    }
                });

                if num_errors == 0 {
                    // Some simple error checking.
                    let span = &variant.properties().prompt.key_span;
                    // Validation already done that the prompt is valid.
                    // We should just ensure that atleast one of the input or output replacers is used.
                    if input_replacers.is_empty() {
                        diag.push_warning(DatamodelWarning::prompt_variable_unused(
                            "Expected prompt to use {#input}",
                            span.clone(),
                        ));
                    }

                    // TODO: We should ensure every enum the class uses is used here.
                    if output_replacers.is_empty() {
                        diag.push_warning(DatamodelWarning::prompt_variable_unused(
                            "Expected prompt to use {#print_type(..)} or {#print_enum(..)} but none was found",
                            span.clone(),
                        ));
                    }

                    // Only in this case update the prompt.
                    vars.insert(
                        variant.id,
                        (input_replacers, output_replacers, chat_replacers),
                    );
                }
            } else {
                diag.push_error(DatamodelError::new_type_not_found_error(
                    variant.function_identifier().name(),
                    self.valid_function_names(),
                    variant.function_identifier().span().clone(),
                ));
            }
        });

        if !diag.has_errors() {
            vars.into_iter().for_each(|(k, v)| {
                self.types.variant_properties.get_mut(&k).unwrap().replacers = v;
            });
        }
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
