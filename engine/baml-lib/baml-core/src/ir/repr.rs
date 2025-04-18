use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use anyhow::{anyhow, Result};
use baml_types::BamlMap;
use baml_types::{
    expr::{self, Expr, ExprMetadata, Name, VarIndex},
    Arrow, BamlValueWithMeta, Constraint, ConstraintLevel, FieldType, JinjaExpression, Resolvable,
    StreamingBehavior, StringOr, TypeValue, UnresolvedValue,
};
use either::Either;
use indexmap::{IndexMap, IndexSet};
use internal_baml_diagnostics::{Diagnostics, Span};
use internal_baml_parser_database::{
    walkers::{
        ClassWalker, ClientWalker, ConfigurationWalker, EnumValueWalker, EnumWalker, ExprFnWalker,
        FieldWalker, FunctionWalker, TemplateStringWalker, TopLevelAssignmentWalker,
        TypeAliasWalker, Walker as AstWalker,
    },
    Attributes, ParserDatabase, PromptAst, RetryPolicyStrategy, TypeWalker,
};

use internal_baml_schema_ast::ast::{
    self, Attribute, FieldArity, SubType, ValExpId, WithAttributes, WithIdentifier, WithName,
    WithSpan,
};
use internal_llm_client::{ClientProvider, ClientSpec, UnresolvedClientProperty};
use serde::Serialize;

use crate::validate::validation_pipeline::validations::expr_typecheck::infer_types_in_context;
use crate::Configuration;

/// This class represents the intermediate representation of the BAML AST.
/// It is a representation of the BAML AST that is easier to work with than the
/// raw BAML AST, and should include all information necessary to generate
/// code in any target language.
#[derive(Debug)]
pub struct IntermediateRepr {
    enums: Vec<Node<Enum>>,
    classes: Vec<Node<Class>>,
    type_aliases: Vec<Node<TypeAlias>>,
    pub functions: Vec<Node<Function>>,
    pub expr_fns: Vec<Node<ExprFunction>>,
    pub toplevel_assignments: Vec<Node<TopLevelAssignment>>,
    clients: Vec<Node<Client>>,
    retry_policies: Vec<Node<RetryPolicy>>,
    template_strings: Vec<Node<TemplateString>>,

    /// Strongly connected components of the dependency graph (finite cycles).
    finite_recursive_cycles: Vec<IndexSet<String>>,

    /// Type alias cycles introduced by lists and maps.
    ///
    /// These are the only allowed cycles, because lists and maps introduce a
    /// level of indirection that makes the cycle finite.
    structural_recursive_alias_cycles: Vec<IndexMap<String, FieldType>>,

    configuration: Configuration,
}

#[derive(Debug)]
pub struct TopLevelAssignment {
    pub name: Node<String>,
    pub expr: Node<Expr<ExprMetadata>>,
}

#[derive(Clone, Debug)]
pub struct ClassConstructor {
    pub class_name: Node<String>,
    pub fields: Vec<Node<ClassConstructorField>>,
}

#[derive(Clone, Debug)]
pub enum ClassConstructorField {
    Named(Node<String>, Node<Expr<ExprMetadata>>),
    Spread(Node<Expr<ExprMetadata>>),
}

impl WithRepr<TopLevelAssignment> for TopLevelAssignmentWalker<'_> {
    fn attributes(&self, _: &ParserDatabase) -> NodeAttributes {
        // TODO: Add attributes.
        NodeAttributes::default()
    }

    fn repr(&self, db: &ParserDatabase) -> Result<TopLevelAssignment> {
        let name = self
            .top_level_assignment()
            .stmt
            .identifier
            .name()
            .to_string();
        let expr = self.top_level_assignment().stmt.body.repr(db)?;
        Ok(TopLevelAssignment {
            name: Node {
                elem: name,
                attributes: NodeAttributes::default(),
            },
            expr: Node {
                elem: expr,
                attributes: NodeAttributes::default(),
            },
        })
    }
}

impl WithRepr<ExprFunction> for ExprFnWalker<'_> {
    fn repr(&self, db: &ParserDatabase) -> Result<ExprFunction> {
        let body = convert_function_body(self.expr_fn().body.to_owned(), db)?;
        let args: Vec<(String, FieldType)> = self
            .expr_fn()
            .args
            .args
            .iter()
            .map(|(arg_name, arg_type)| {
                arg_type
                    .field_type
                    .repr(db)
                    .map(|ty| (arg_name.to_string(), ty))
            })
            .collect::<Result<Vec<_>>>()?;
        let arg_names = self
            .expr_fn()
            .args
            .args
            .iter()
            .map(|(arg_name, _arg_type)| arg_name.to_string())
            .collect::<Vec<String>>();
        let closed_body = arg_names.iter().enumerate().fold(body, |acc, (ind, name)| {
            acc.close(
                &VarIndex {
                    de_bruijn: 0,
                    tuple: ind as u32,
                },
                name,
            )
        });
        let tests = self
            .walk_tests()
            .map(|e| e.node(db))
            .collect::<Result<Vec<_>>>()?;
        let arg_types = args
            .iter()
            .map(|(_, arg_type)| arg_type.clone())
            .collect::<Vec<_>>();
        let arity = arg_types.len();
        let return_type = self
            .expr_fn()
            .return_type
            .clone()
            .map(|ret| ret.repr(db))
            .transpose()?
            .ok_or(anyhow::anyhow!(
                "Expression functions must have a return type"
            ))?;
        let lambda_type = FieldType::Arrow(Box::new(Arrow {
            param_types: arg_types,
            return_type: return_type.clone(),
        }));
        let expr_fn = ExprFunction {
            name: self.expr_fn().name.to_string(),
            inputs: args,
            output: return_type,
            expr: Expr::Lambda(
                arity,
                Arc::new(closed_body),
                (self.expr_fn().span.clone(), Some(lambda_type)),
            ),
            tests,
        };
        Ok(expr_fn)
    }
}

fn weird_default() -> FieldType {
    FieldType::Primitive(TypeValue::Null)
}

impl WithRepr<Function> for ExprFnWalker<'_> {
    fn repr(&self, db: &ParserDatabase) -> Result<Function> {
        // TODO: Drop weird default (replace by better validation).
        let body = convert_function_body(self.expr_fn().body.to_owned(), db)?;
        let args = self
            .expr_fn()
            .args
            .args
            .iter()
            .map(|(arg_name, arg_type)| {
                let ty = arg_type.field_type.repr(db)?;
                Ok((arg_name.to_string(), ty))
            })
            .collect::<Result<_>>()?;
        let return_ty = self
            .expr_fn()
            .return_type
            .as_ref()
            .ok_or(anyhow::anyhow!(
                "Expression functions must have return type."
            ))?
            .repr(db)?;
        let function = Function {
            name: self.expr_fn().name.to_string(),
            inputs: args,
            output: return_ty,
            configs: vec![],
            default_config: "".to_string(),
            tests: vec![],
        };
        Ok(function)
    }
}

/// Convert a function body to an expression.
///
/// The function body is a list of statements, which are let bindings.
/// We fold the let bindings into a single expression.
/// {
///   let x = 1;
///   let y = x;
///   y
/// }
/// =>
/// Let "x" 1 (Let "y" x (y))
fn convert_function_body(
    function_body: ast::ExpressionBlock,
    db: &ParserDatabase,
) -> Result<Expr<ExprMetadata>> {
    function_body.expr.repr(db).map(|fn_body| {
        let expr = function_body
            .stmts
            .iter()
            .fold(fn_body, |acc, stmt| match stmt.body.repr(db) {
                Ok(stmt_expr) => Expr::Let(
                    stmt.identifier.name().to_string(),
                    Arc::new(stmt_expr),
                    Arc::new(acc),
                    (stmt.body.span().clone(), None),
                ),
                Err(e) => acc,
            });
        expr
    })
}

impl WithRepr<Expr<ExprMetadata>> for ast::Expression {
    fn repr(&self, db: &ParserDatabase) -> Result<Expr<ExprMetadata>> {
        match self {
            ast::Expression::BoolValue(val, span) => Ok(Expr::Atom(BamlValueWithMeta::Bool(
                *val,
                (span.clone(), Some(FieldType::Primitive(TypeValue::Bool))),
            ))),
            ast::Expression::NumericValue(val, span) => val
                .parse::<i64>()
                .map(|v| {
                    Expr::Atom(BamlValueWithMeta::Int(
                        v,
                        (span.clone(), Some(FieldType::Primitive(TypeValue::Int))),
                    ))
                })
                .or_else(|_| {
                    val.parse::<f64>()
                        .map(|v| {
                            Expr::Atom(BamlValueWithMeta::Float(
                                v,
                                (span.clone(), Some(FieldType::Primitive(TypeValue::Float))),
                            ))
                        })
                        .or_else(|_| Err(anyhow!("Invalid numeric value: {}", val)))
                }),
            ast::Expression::StringValue(val, span) => Ok(Expr::Atom(BamlValueWithMeta::String(
                val.to_string(),
                (span.clone(), Some(FieldType::Primitive(TypeValue::String))),
            ))),
            ast::Expression::RawStringValue(val) => Ok(Expr::Atom(BamlValueWithMeta::String(
                val.value().to_string(),
                (
                    val.span().clone(),
                    Some(FieldType::Primitive(TypeValue::String)),
                ),
            ))),
            ast::Expression::JinjaExpressionValue(val, span) => {
                Ok(Expr::Atom(BamlValueWithMeta::String(
                    val.to_string(),
                    (span.clone(), Some(FieldType::Primitive(TypeValue::String))),
                )))
            }
            ast::Expression::Array(vals, span) => {
                let new_items = vals
                    .iter()
                    .map(|v| v.repr(db))
                    .collect::<Result<Vec<_>>>()?;
                let mut item_types = new_items
                    .iter()
                    .filter_map(|v| v.meta().1.clone())
                    .collect::<Vec<_>>();
                item_types.dedup();
                let item_type = match item_types.len() {
                    0 => None,
                    1 => Some(item_types[0].clone()),
                    _ => Some(FieldType::Union(item_types)),
                };
                let list_type = item_type.map(|t| FieldType::List(Box::new(t)));
                Ok(Expr::List(new_items, (span.clone(), list_type)))
            }
            ast::Expression::Map(vals, span) => {
                let new_items = vals
                    .iter()
                    .map(|(k, v)| v.repr(db).map(|v2| (k.to_string(), v2)))
                    .collect::<Result<IndexMap<_, _>>>()?;
                let mut item_types = new_items
                    .iter()
                    .filter_map(|v| v.1.meta().1.clone())
                    .collect::<Vec<_>>();
                item_types.dedup();
                let item_type = match item_types.len() {
                    0 => None,
                    1 => Some(item_types[0].clone()),
                    _ => Some(FieldType::Union(item_types)),
                };
                // TODO: Is this correct?
                let key_type = FieldType::Primitive(TypeValue::String);
                let map_type = item_type.map(|t| FieldType::Map(Box::new(key_type), Box::new(t)));
                Ok(Expr::Map(new_items, (span.clone(), map_type)))
            }
            ast::Expression::Identifier(id) => Ok(Expr::FreeVar(
                id.name().to_string(),
                (id.span().clone(), None),
            )),

            ast::Expression::Lambda(args, body, span) => {
                let args = args
                    .arguments
                    .iter()
                    .filter_map(|arg| arg.value.as_string_value().map(|v| v.0.to_string()))
                    .collect::<Vec<String>>();
                let arity = args.len();
                let body = convert_function_body(*body.to_owned(), db)?;
                let closed_body = args.iter().enumerate().fold(body, |acc, (ind, arg_name)| {
                    acc.close(
                        &VarIndex {
                            de_bruijn: 0,
                            tuple: ind as u32,
                        },
                        arg_name,
                    )
                });
                Ok(Expr::Lambda(
                    arity,
                    Arc::new(closed_body),
                    (span.clone(), None),
                ))
            }
            ast::Expression::FnApp(func, args, span) => {
                let func = Expr::FreeVar(func.name().to_string(), (func.span().clone(), None));
                let args = args.iter().map(|arg| arg.repr(db)).collect::<Result<_>>()?;
                Ok(Expr::App(
                    Arc::new(func),
                    Arc::new(Expr::ArgsTuple(args, (span.clone(), None))), // TODO: We don't really have a span for the ArgsTuple, so we're using the one for the whole FnApp.
                    (span.clone(), None),
                ))
            }
            ast::Expression::ClassConstructor(
                ast::ClassConstructor { class_name, fields },
                span,
            ) => {
                let mut new_fields = BamlMap::new();
                let mut spread = None;
                for f in fields {
                    match f {
                        ast::ClassConstructorField::Named(name, expr) => {
                            new_fields.insert(name.to_string(), expr.repr(db)?);
                        }
                        ast::ClassConstructorField::Spread(expr) => {
                            spread = Some(Box::new(expr.repr(db)?));
                        }
                    }
                }
                Ok(Expr::ClassConstructor {
                    name: class_name.name().to_string(),
                    fields: new_fields,
                    spread,
                    meta: (
                        span.clone(),
                        Some(FieldType::Class(class_name.name().to_string())),
                    ),
                })
            }
            ast::Expression::ExprBlock(block, span) => {
                // We use "function_body" and "expr_block" interchangeably.
                // This may need to be revisited?
                let body = convert_function_body(block.clone(), db)?;
                Ok(body)
            }
        }
    }
}

/// A generic walker. Only walkers instantiated with a concrete ID type (`I`) are useful.
#[derive(Clone, Copy)]
pub struct Walker<'ir, I> {
    /// The IR being traversed.
    pub ir: &'ir IntermediateRepr,
    /// The identifier of the focused element.
    pub item: I,
}

impl IntermediateRepr {
    pub fn create_empty() -> IntermediateRepr {
        IntermediateRepr {
            enums: vec![],
            classes: vec![],
            type_aliases: vec![],
            finite_recursive_cycles: vec![],
            structural_recursive_alias_cycles: vec![],
            functions: vec![],
            expr_fns: vec![],
            toplevel_assignments: vec![],
            clients: vec![],
            retry_policies: vec![],
            template_strings: vec![],
            configuration: Configuration::new(),
        }
    }

    pub fn configuration(&self) -> &Configuration {
        &self.configuration
    }

    pub fn required_env_vars(&self) -> HashSet<String> {
        // TODO: We should likely check the full IR.
        let mut env_vars = HashSet::new();

        for client in self.walk_clients() {
            client.required_env_vars().iter().for_each(|v| {
                env_vars.insert(v.to_string());
            });
        }

        // self.walk_functions().filter_map(
        //     |f| f.client_name()
        // ).map(|c| c.required_env_vars())

        // // for any functions, check for shorthand env vars
        // self.functions
        //     .iter()
        //     .filter_map(|f| f.elem.configs())
        //     .into_iter()
        //     .flatten()
        //     .flat_map(|(expr)| expr.client.required_env_vars())
        //     .collect()
        env_vars
    }

    /// Returns a list of all the recursive cycles in the IR.
    ///
    /// Each cycle is represented as a set of strings, where each string is the
    /// name of a class.
    pub fn finite_recursive_cycles(&self) -> &[IndexSet<String>] {
        &self.finite_recursive_cycles
    }

    pub fn structural_recursive_alias_cycles(&self) -> &[IndexMap<String, FieldType>] {
        &self.structural_recursive_alias_cycles
    }

    pub fn walk_enums(&self) -> impl ExactSizeIterator<Item = Walker<'_, &Node<Enum>>> {
        self.enums.iter().map(|e| Walker { ir: self, item: e })
    }

    pub fn walk_classes(&self) -> impl ExactSizeIterator<Item = Walker<'_, &Node<Class>>> {
        self.classes.iter().map(|e| Walker { ir: self, item: e })
    }

    pub fn walk_type_aliases(&self) -> impl ExactSizeIterator<Item = Walker<'_, &Node<TypeAlias>>> {
        self.type_aliases
            .iter()
            .map(|e| Walker { ir: self, item: e })
    }

    // TODO: Exact size Iterator + Node<>?
    pub fn walk_alias_cycles(&self) -> impl Iterator<Item = Walker<'_, (&String, &FieldType)>> {
        self.structural_recursive_alias_cycles
            .iter()
            .flatten()
            .map(|e| Walker { ir: self, item: e })
    }

    pub fn function_names(&self) -> impl ExactSizeIterator<Item = &str> {
        self.functions.iter().map(|f| f.elem.name())
    }

    pub fn walk_functions(&self) -> impl ExactSizeIterator<Item = Walker<'_, &Node<Function>>> {
        self.functions.iter().map(|e| Walker { ir: self, item: e })
    }

    // TODO: This is a quick workaround in order to make expr_fns compatible
    // with LLM functions for the purpose of listing functions and test
    // cases in the playground.
    pub fn expr_fns_as_functions(&self) -> Vec<Node<Function>> {
        self.expr_fns
            .iter()
            .map(|efn| Node {
                elem: efn.elem.pretend_to_be_llm_function(),
                attributes: efn.attributes.clone(),
            })
            .collect::<Vec<_>>()
    }

    pub fn walk_toplevel_assignments(
        &self,
    ) -> impl ExactSizeIterator<Item = Walker<'_, &Node<TopLevelAssignment>>> {
        self.toplevel_assignments
            .iter()
            .map(|e| Walker { ir: self, item: e })
    }

    pub fn walk_expr_fns(&self) -> impl ExactSizeIterator<Item = Walker<'_, &Node<ExprFunction>>> {
        self.expr_fns.iter().map(|e| Walker { ir: self, item: e })
    }

    pub fn walk_tests(
        &self,
    ) -> impl Iterator<Item = Walker<'_, (&Node<Function>, &Node<TestCase>)>> {
        self.functions.iter().flat_map(move |f| {
            f.elem.tests().iter().map(move |t| Walker {
                ir: self,
                item: (f, t),
            })
        })
    }

    pub fn walk_clients(&self) -> impl ExactSizeIterator<Item = Walker<'_, &Node<Client>>> {
        self.clients.iter().map(|e| Walker { ir: self, item: e })
    }

    pub fn walk_template_strings(
        &self,
    ) -> impl ExactSizeIterator<Item = Walker<'_, &Node<TemplateString>>> {
        self.template_strings
            .iter()
            .map(|e| Walker { ir: self, item: e })
    }

    #[allow(dead_code)]
    pub fn walk_retry_policies(
        &self,
    ) -> impl ExactSizeIterator<Item = Walker<'_, &Node<RetryPolicy>>> {
        self.retry_policies
            .iter()
            .map(|e| Walker { ir: self, item: e })
    }

    pub fn from_parser_database(
        db: &ParserDatabase,
        configuration: Configuration,
    ) -> Result<IntermediateRepr> {
        // TODO: We're iterating over the AST tops once for every property in
        // the IR. Easy performance optimization here by iterating only one time
        // and distributing the tops to the appropriate IR properties.
        let mut repr = IntermediateRepr {
            enums: db
                .walk_enums()
                .map(|e| e.node(db))
                .collect::<Result<Vec<_>>>()?,
            classes: db
                .walk_classes()
                .map(|e| e.node(db))
                .collect::<Result<Vec<_>>>()?,
            type_aliases: db
                .walk_type_aliases()
                .map(|e| e.node(db))
                .collect::<Result<Vec<_>>>()?,
            finite_recursive_cycles: db
                .finite_recursive_cycles()
                .iter()
                .map(|ids| {
                    ids.iter()
                        .map(|id| db.ast()[*id].name().to_string())
                        .collect()
                })
                .collect(),
            structural_recursive_alias_cycles: {
                let mut recursive_aliases = vec![];
                for cycle in db.recursive_alias_cycles() {
                    let mut component = IndexMap::new();
                    for id in cycle {
                        let alias = &db.ast()[*id];
                        component.insert(alias.name().to_string(), alias.value.repr(db)?);
                    }
                    recursive_aliases.push(component);
                }
                recursive_aliases
            },
            functions: db
                .walk_functions()
                .map(|e| e.node(db))
                .collect::<Result<Vec<_>>>()?,
            expr_fns: db
                .walk_expr_fns()
                .map(|e| e.node(db))
                .collect::<Result<Vec<_>>>()?,
            toplevel_assignments: db
                .walk_toplevel_assignments()
                .map(|e| e.node(db))
                .collect::<Result<Vec<_>>>()?,
            clients: db
                .walk_clients()
                .map(|e| e.node(db))
                .collect::<Result<Vec<_>>>()?,
            retry_policies: db
                .walk_retry_policies()
                .map(|e| WithRepr::<RetryPolicy>::node(&e, db))
                .collect::<Result<Vec<_>>>()?,
            template_strings: db
                .walk_templates()
                .map(|e| e.node(db))
                .collect::<Result<Vec<_>>>()?,
            configuration,
        };

        // Sort each item by name.
        repr.enums.sort_by(|a, b| a.elem.name.cmp(&b.elem.name));
        repr.classes.sort_by(|a, b| a.elem.name.cmp(&b.elem.name));
        repr.functions
            .sort_by(|a, b| a.elem.name().cmp(b.elem.name()));
        repr.clients.sort_by(|a, b| a.elem.name.cmp(&b.elem.name));
        repr.retry_policies
            .sort_by(|a, b| a.elem.name.0.cmp(&b.elem.name.0));

        // TODO: Necessary?
        for expr_fn in repr.expr_fns.iter_mut() {
            let expr = expr_fn.elem.expr.clone();
            let inferred_expr = infer_types_in_context(&mut HashMap::new(), Arc::new(expr));
            expr_fn.elem.expr = Arc::unwrap_or_clone(inferred_expr);
        }

        Ok(repr)
    }

    /// TODO: #1343 Temporary solution until we implement scoping in the AST.
    pub fn type_builder_entries_from_scoped_db(
        scoped_db: &ParserDatabase,
        global_db: &ParserDatabase,
    ) -> Result<(
        Vec<Node<Class>>,
        Vec<Node<Enum>>,
        Vec<Node<TypeAlias>>,
        Vec<IndexSet<String>>,
        Vec<IndexMap<String, FieldType>>,
    )> {
        let classes = scoped_db
            .walk_classes()
            .filter(|c| {
                scoped_db.ast()[c.id].is_dynamic_type_def
                    || global_db.find_type_by_str(c.name()).is_none()
            })
            .map(|c| c.node(scoped_db))
            .collect::<Result<Vec<Node<Class>>>>()?;

        let enums = scoped_db
            .walk_enums()
            .filter(|e| {
                scoped_db.ast()[e.id].is_dynamic_type_def
                    || global_db.find_type_by_str(e.name()).is_none()
            })
            .map(|e| e.node(scoped_db))
            .collect::<Result<Vec<Node<Enum>>>>()?;

        let type_aliases = scoped_db
            .walk_type_aliases()
            .filter(|a| global_db.find_type_by_str(a.name()).is_none())
            .map(|a| a.node(scoped_db))
            .collect::<Result<Vec<Node<TypeAlias>>>>()?;

        let recursive_classes = scoped_db
            .finite_recursive_cycles()
            .iter()
            .map(|ids| {
                ids.iter()
                    .map(|id| {
                        let name = scoped_db.ast()[*id].name();
                        if name.starts_with(ast::DYNAMIC_TYPE_NAME_PREFIX) {
                            name.strip_prefix(ast::DYNAMIC_TYPE_NAME_PREFIX)
                                .unwrap()
                                .to_string()
                        } else {
                            name.to_string()
                        }
                    })
                    .collect()
            })
            .collect();

        let mut recursive_aliases = vec![];
        for cycle in scoped_db.recursive_alias_cycles() {
            let mut component = IndexMap::new();
            for id in cycle {
                let alias = &scoped_db.ast()[*id];
                // Those are global cycles, skip.
                if global_db.find_type_by_str(alias.name()).is_some() {
                    continue;
                }
                // Cycles defined in the scoped test type builder block.
                component.insert(alias.name().to_string(), alias.value.repr(scoped_db)?);
            }
            recursive_aliases.push(component);
        }

        Ok((
            classes,
            enums,
            type_aliases,
            recursive_classes,
            recursive_aliases,
        ))
    }
}

// TODO:
//
//   [x] clients - need to finish expressions
//   [x] metadata per node (attributes, spans, etc)
//           block-level attributes on enums, classes
//           field-level attributes on enum values, class fields
//           overrides can only exist in impls
//   [x] FieldArity (optional / required) needs to be handled
//   [x] other types of identifiers?
//   [ ] `baml update` needs to update lockfile right now
//          but baml CLI is installed globally
//   [ ] baml configuration - retry policies, generator, etc
//          [x] retry policies
//   [x] rename lockfile/mod.rs to ir/mod.rs
//   [x] wire Result<> type through, need this to be more sane

#[derive(Clone, Debug)]
pub struct NodeAttributes {
    /// Map of attributes on the corresponding IR node.
    ///
    /// Some follow special conventions:
    ///
    ///   - @skip becomes ("skip", bool)
    ///   - @alias(...) becomes ("alias", ...)
    meta: IndexMap<String, UnresolvedValue<()>>,

    pub constraints: Vec<Constraint>,

    /// Total span of the Node.
    ///
    /// ```ignore
    /// <SPAN_START> class Example {
    ///     a string
    ///     b int
    /// } <SPAN_END>
    /// ```
    ///
    /// TODO: Create an `ir::Span` struct and use it to store all the spans
    /// we've defined here. Something like:
    ///
    /// ```ignore
    /// struct Span {
    ///     total: ast::Span,
    ///     identifier: ast::Span,
    ///     symbols: HashMap<String, ast::Span>,
    /// }
    /// ```
    pub span: Option<ast::Span>,

    /// Span of the identifier only.
    ///
    /// ```ignore
    /// class <SPAN_START> Example <SPAN_END> {
    ///     a string
    ///     b int
    /// }
    /// ```
    ///
    /// In the case of fields this is the field name span.
    ///
    /// ```ignore
    /// class Example {
    ///     <SPAN_START> a <SPAN_END> string
    ///     b int
    /// }
    /// ```
    pub identifier_span: Option<ast::Span>,

    /// Other important spans for renaming or similar features.
    ///
    /// For example, imagine we have a union:
    ///
    /// ```ignore
    /// class Example {
    ///     union int | OtherClass | string | OtherClass // Yes it can appear multiple times
    /// }
    /// ```
    ///
    /// And we want to rename the `OtherClass` type. We can't do that unless we
    /// know the exact span of the symbol in the union.
    ///
    /// We could store this in [`FieldType::WithMetadata`] but currently that
    /// variant only stores data attached to the field by the user (contraints,
    /// streaming behavior), and it would also require every single
    /// [`FieldType`] in the IR to be [`FieldType::WithMetadata`] which might
    /// break some code or match statements elsewhere.
    pub symbol_spans: HashMap<String, Vec<ast::Span>>,
}

impl NodeAttributes {
    pub fn get(&self, key: &str) -> Option<&UnresolvedValue<()>> {
        self.meta.get(key)
    }

    pub fn streaming_behavior(&self) -> StreamingBehavior {
        fn is_some_true(maybe_value: Option<&UnresolvedValue<()>>) -> bool {
            match maybe_value {
                Some(Resolvable::Bool(true, _)) => true,
                _ => false,
            }
        }
        StreamingBehavior {
            done: is_some_true(self.get("stream.done")),
            state: is_some_true(self.get("stream.with_state")),
        }
    }
}

impl Default for NodeAttributes {
    fn default() -> Self {
        NodeAttributes {
            meta: IndexMap::new(),
            constraints: Vec::new(),
            span: None,
            identifier_span: None,
            symbol_spans: HashMap::new(),
        }
    }
}

fn to_ir_attributes(
    db: &ParserDatabase,
    maybe_ast_attributes: Option<&Attributes>,
) -> (IndexMap<String, UnresolvedValue<()>>, Vec<Constraint>) {
    let Some(attributes) = maybe_ast_attributes else {
        return (IndexMap::new(), Vec::new());
    };

    let Attributes {
        description,
        alias,
        dynamic_type,
        skip,
        constraints,
        streaming_done,
        streaming_needed,
        streaming_state,
    } = attributes;

    let description = description
        .as_ref()
        .map(|d| ("description".to_string(), d.without_meta()));

    let alias = alias
        .as_ref()
        .map(|v| ("alias".to_string(), v.without_meta()));

    let dynamic_type = dynamic_type.as_ref().and_then(|v| {
        if *v {
            Some(("dynamic_type".to_string(), UnresolvedValue::Bool(true, ())))
        } else {
            None
        }
    });
    let skip = skip.as_ref().and_then(|v| {
        if *v {
            Some(("skip".to_string(), UnresolvedValue::Bool(true, ())))
        } else {
            None
        }
    });
    let streaming_done = streaming_done.as_ref().and_then(|v| {
        if *v {
            Some(("stream.done".to_string(), UnresolvedValue::Bool(true, ())))
        } else {
            None
        }
    });
    let streaming_needed = streaming_needed.as_ref().and_then(|v| {
        if *v {
            Some((
                "stream.not_null".to_string(),
                UnresolvedValue::Bool(true, ()),
            ))
        } else {
            None
        }
    });
    let streaming_state = streaming_state.as_ref().and_then(|v| {
        if *v {
            Some((
                "stream.with_state".to_string(),
                UnresolvedValue::Bool(true, ()),
            ))
        } else {
            None
        }
    });

    let meta = vec![
        description,
        alias,
        dynamic_type,
        skip,
        streaming_done,
        streaming_needed,
        streaming_state,
    ]
    .into_iter()
    .filter_map(|s| s)
    .collect();

    (meta, constraints.clone())
}

/// Nodes allow attaching metadata to a given IR entity: attributes, source location, etc
#[derive(Clone, Debug)]
pub struct Node<T> {
    pub attributes: NodeAttributes,
    pub elem: T,
}

/// Implement this for every node in the IR AST, where T is the type of IR node
pub trait WithRepr<T> {
    /// Represents block or field attributes - @@ for enums and classes, @ for enum values and class fields
    fn attributes(&self, _: &ParserDatabase) -> NodeAttributes {
        NodeAttributes::default()
    }

    fn repr(&self, db: &ParserDatabase) -> Result<T>;

    fn node(&self, db: &ParserDatabase) -> Result<Node<T>> {
        Ok(Node {
            elem: self.repr(db)?,
            attributes: self.attributes(db),
        })
    }
}

fn type_with_arity(t: FieldType, arity: &FieldArity) -> FieldType {
    match arity {
        FieldArity::Required => t,
        FieldArity::Optional => FieldType::Optional(Box::new(t)),
    }
}

impl WithRepr<FieldType> for ast::FieldType {
    // TODO: (Greg) This code only extracts constraints, and ignores any
    // other types of attributes attached to the type directly.
    fn attributes(&self, _db: &ParserDatabase) -> NodeAttributes {
        let constraints = self
            .attributes()
            .iter()
            .filter_map(|attr| {
                let level = match attr.name.to_string().as_str() {
                    "assert" => Some(ConstraintLevel::Assert),
                    "check" => Some(ConstraintLevel::Check),
                    _ => None,
                }?;
                let (label, expression) = match attr.arguments.arguments.as_slice() {
                    [arg1, arg2] => match (arg1.clone().value, arg2.clone().value) {
                        (
                            ast::Expression::Identifier(ast::Identifier::Local(s, _)),
                            ast::Expression::JinjaExpressionValue(j, _),
                        ) => Some((Some(s), j)),
                        _ => None,
                    },
                    [arg1] => match arg1.clone().value {
                        ast::Expression::JinjaExpressionValue(JinjaExpression(j), _) => {
                            Some((None, JinjaExpression(j.clone())))
                        }
                        _ => None,
                    },
                    _ => None,
                }?;
                Some(Constraint {
                    level,
                    expression,
                    label,
                })
            })
            .collect::<Vec<Constraint>>();
        let mut meta = IndexMap::new();
        if self
            .attributes()
            .iter()
            .find(|Attribute { name, .. }| name.name() == "stream.done")
            .is_some()
        {
            let val: UnresolvedValue<()> = Resolvable::Bool(true, ());
            meta.insert("stream.done".to_string(), val);
        }
        if self
            .attributes()
            .iter()
            .find(|Attribute { name, .. }| name.name() == "stream.with_state")
            .is_some()
        {
            let val: UnresolvedValue<()> = Resolvable::Bool(true, ());
            meta.insert("stream.with_state".to_string(), val);
        }

        let mut symbol_spans = HashMap::new();

        let mut stack = vec![self];
        while let Some(item) = stack.pop() {
            match item {
                // Base case, store span.
                ast::FieldType::Symbol(_, idn, ..) => {
                    if !symbol_spans.contains_key(idn.name()) {
                        symbol_spans.insert(idn.name().to_string(), vec![idn.span().clone()]);
                    } else {
                        symbol_spans
                            .get_mut(idn.name())
                            .unwrap()
                            .push(idn.span().clone());
                    }
                }
                // Recurse.
                ast::FieldType::List(_, ft, ..) => stack.push(ft),
                ast::FieldType::Map(_, kv, ..) => {
                    let (k, v) = &**kv;
                    stack.push(k);
                    stack.push(v);
                }
                ast::FieldType::Union(_, items, ..) | ast::FieldType::Tuple(_, items, ..) => {
                    stack.extend(items.iter());
                }
                // No identifiers here.
                ast::FieldType::Primitive(..) | ast::FieldType::Literal(..) => {}
            }
        }

        let attributes = NodeAttributes {
            meta,
            constraints,
            span: Some(self.span().clone()),
            identifier_span: None,
            symbol_spans,
        };

        attributes
    }

    fn repr(&self, db: &ParserDatabase) -> Result<FieldType> {
        let attributes = WithRepr::attributes(self, db);
        let has_constraints = !attributes.constraints.is_empty();
        let streaming_behavior = attributes.streaming_behavior();
        let has_special_streaming_behavior = streaming_behavior != StreamingBehavior::default();
        let base = match self {
            ast::FieldType::Primitive(arity, typeval, ..) => {
                let repr = FieldType::Primitive(*typeval);
                if arity.is_optional() {
                    FieldType::Optional(Box::new(repr))
                } else {
                    repr
                }
            }
            ast::FieldType::Literal(arity, literal_value, ..) => {
                let repr = FieldType::Literal(literal_value.clone());
                if arity.is_optional() {
                    FieldType::Optional(Box::new(repr))
                } else {
                    repr
                }
            }
            ast::FieldType::Symbol(arity, idn, ..) => type_with_arity(
                match db.find_type(idn) {
                    Some(TypeWalker::Class(class_walker)) => {
                        let base_class = FieldType::Class(class_walker.name().to_string());
                        match class_walker.get_constraints(SubType::Class) {
                            Some(constraints) if !constraints.is_empty() => {
                                FieldType::WithMetadata {
                                    base: Box::new(base_class),
                                    constraints,
                                    streaming_behavior: streaming_behavior.clone(),
                                }
                            }
                            _ => base_class,
                        }
                    }
                    Some(TypeWalker::Enum(enum_walker)) => {
                        let base_type = FieldType::Enum(enum_walker.name().to_string());
                        match enum_walker.get_constraints(SubType::Enum) {
                            Some(constraints) if !constraints.is_empty() => {
                                FieldType::WithMetadata {
                                    base: Box::new(base_type),
                                    constraints,
                                    streaming_behavior: streaming_behavior.clone(),
                                }
                            }
                            _ => base_type,
                        }
                    }
                    Some(TypeWalker::TypeAlias(alias_walker)) => {
                        if db.is_recursive_type_alias(&alias_walker.id) {
                            FieldType::RecursiveTypeAlias(alias_walker.name().to_string())
                        } else {
                            alias_walker.resolved().to_owned().repr(db)?
                        }
                    }

                    None => {
                        return Err(anyhow!(
                            "Field type uses unresolvable local identifier {}",
                            idn
                        ))
                    }
                },
                arity,
            ),
            ast::FieldType::List(arity, ft, dims, ..) => {
                // NB: potential bug: this hands back a 1D list when dims == 0
                let mut repr = FieldType::List(Box::new(ft.repr(db)?));

                for _ in 1u32..*dims {
                    repr = FieldType::list(repr);
                }

                if arity.is_optional() {
                    repr = FieldType::optional(repr);
                }

                repr
            }
            ast::FieldType::Map(arity, kv, ..) => {
                // NB: we can't just unpack (*kv) into k, v because that would require a move/copy
                let mut repr =
                    FieldType::Map(Box::new((kv).0.repr(db)?), Box::new((kv).1.repr(db)?));

                if arity.is_optional() {
                    repr = FieldType::optional(repr);
                }

                repr
            }
            ast::FieldType::Union(arity, t, ..) => {
                // NB: preempt union flattening by mixing arity into union types
                let mut types = t.iter().map(|ft| ft.repr(db)).collect::<Result<Vec<_>>>()?;

                if arity.is_optional() {
                    types.push(FieldType::Primitive(baml_types::TypeValue::Null));
                }

                FieldType::Union(types)
            }
            ast::FieldType::Tuple(arity, t, ..) => type_with_arity(
                FieldType::Tuple(t.iter().map(|ft| ft.repr(db)).collect::<Result<Vec<_>>>()?),
                arity,
            ),
        };

        let use_metadata = has_constraints || has_special_streaming_behavior;
        let with_constraints = if use_metadata {
            FieldType::WithMetadata {
                base: Box::new(base.clone()),
                constraints: attributes.constraints,
                streaming_behavior,
            }
        } else {
            base
        };
        Ok(with_constraints)
    }
}

type TemplateStringId = String;

#[derive(Debug)]
pub struct TemplateString {
    pub name: TemplateStringId,
    pub params: Vec<Field>,
    pub content: String,
}

impl WithRepr<TemplateString> for TemplateStringWalker<'_> {
    fn attributes(&self, _: &ParserDatabase) -> NodeAttributes {
        NodeAttributes {
            meta: Default::default(),
            constraints: Vec::new(),
            span: Some(self.span().clone()),
            identifier_span: None,
            symbol_spans: HashMap::new(),
        }
    }

    fn repr(&self, _db: &ParserDatabase) -> Result<TemplateString> {
        Ok(TemplateString {
            name: self.name().to_string(),
            params: self.ast_node().input().map_or(vec![], |e| {
                let ast::BlockArgs { args, .. } = e;
                args.iter()
                    .filter_map(|(id, arg)| {
                        arg.field_type
                            .node(_db)
                            .map(|f| Field {
                                name: id.name().to_string(),
                                r#type: f,
                                docstring: None,
                            })
                            .ok()
                    })
                    .collect::<Vec<_>>()
            }),
            content: self.template_string().to_string(),
        })
    }
}
type EnumId = String;

#[derive(Clone, serde::Serialize, Debug)]
pub struct EnumValue(pub String);

#[derive(Clone, Debug)]
pub struct Enum {
    pub name: EnumId,
    pub values: Vec<(Node<EnumValue>, Option<Docstring>)>,
    /// Docstring.
    pub docstring: Option<Docstring>,
}

impl WithRepr<EnumValue> for EnumValueWalker<'_> {
    fn attributes(&self, db: &ParserDatabase) -> NodeAttributes {
        let (meta, constraints) = to_ir_attributes(db, self.get_default_attributes());
        let attributes = NodeAttributes {
            meta,
            constraints,
            span: Some(self.span().clone()),
            identifier_span: Some(self.span().clone()),
            symbol_spans: HashMap::new(),
        };

        attributes
    }

    fn repr(&self, _db: &ParserDatabase) -> Result<EnumValue> {
        Ok(EnumValue(self.name().to_string()))
    }
}

impl WithRepr<Enum> for EnumWalker<'_> {
    fn attributes(&self, db: &ParserDatabase) -> NodeAttributes {
        let (meta, constraints) = to_ir_attributes(db, self.get_default_attributes(SubType::Enum));
        let attributes = NodeAttributes {
            meta,
            constraints,
            span: Some(self.span().clone()),
            identifier_span: Some(self.identifier().span().clone()),
            symbol_spans: HashMap::new(),
        };

        attributes
    }

    fn repr(&self, db: &ParserDatabase) -> Result<Enum> {
        Ok(Enum {
            // TODO: #1343 Temporary solution until we implement scoping in the AST.
            name: if self.ast_type_block().is_dynamic_type_def {
                self.name()
                    .strip_prefix(ast::DYNAMIC_TYPE_NAME_PREFIX)
                    .unwrap()
                    .to_string()
            } else {
                self.name().to_string()
            },
            values: self
                .values()
                .map(|w| {
                    w.node(db)
                        .map(|v| (v, w.documentation().map(|s| Docstring(s.to_string()))))
                })
                .collect::<Result<Vec<_>, _>>()?,
            docstring: self.get_documentation().map(Docstring),
        })
    }
}

#[derive(Clone, serde::Serialize, Debug)]
pub struct Docstring(pub String);

#[derive(Clone, Debug)]
pub struct Field {
    pub name: String,
    pub r#type: Node<FieldType>,
    pub docstring: Option<Docstring>,
}

impl WithRepr<Field> for FieldWalker<'_> {
    fn attributes(&self, db: &ParserDatabase) -> NodeAttributes {
        let (meta, constraints) = to_ir_attributes(db, self.get_default_attributes());
        let attributes = NodeAttributes {
            meta,
            constraints,
            span: Some(self.span().clone()),
            identifier_span: Some(self.ast_field().identifier().span().clone()),
            symbol_spans: HashMap::new(),
        };

        attributes
    }

    fn repr(&self, db: &ParserDatabase) -> Result<Field> {
        let ast_field_type = self.ast_field().expr.as_ref().ok_or(anyhow!(
            "Internal error occurred while resolving repr of field {:?}",
            self.name(),
        ))?;
        let field_type_attributes = WithRepr::attributes(ast_field_type, db);
        let field_type = ast_field_type.repr(db)?;
        Ok(Field {
            name: self.name().to_string(),
            r#type: Node {
                elem: field_type,
                attributes: field_type_attributes,
            },
            docstring: self.get_documentation().map(Docstring),
        })
    }
}

type ClassId = String;

/// A BAML Class.
#[derive(Clone, Debug)]
pub struct Class {
    /// User defined class name.
    pub name: ClassId,

    /// Fields of the class.
    pub static_fields: Vec<Node<Field>>,

    /// Parameters to the class definition.
    pub inputs: Vec<(String, FieldType)>,

    /// Docstring.
    pub docstring: Option<Docstring>,
}

impl WithRepr<Class> for ClassWalker<'_> {
    fn attributes(&self, db: &ParserDatabase) -> NodeAttributes {
        let default_attributes = self.get_default_attributes(SubType::Class);
        let (meta, constraints) = to_ir_attributes(db, default_attributes);
        let attributes = NodeAttributes {
            meta,
            constraints,
            span: Some(self.span().clone()),
            identifier_span: Some(self.identifier().span().clone()),
            symbol_spans: HashMap::new(),
        };

        attributes
    }

    fn repr(&self, db: &ParserDatabase) -> Result<Class> {
        Ok(Class {
            // TODO: #1343 Temporary solution until we implement scoping in the AST.
            name: if self.ast_type_block().is_dynamic_type_def {
                self.name()
                    .strip_prefix(ast::DYNAMIC_TYPE_NAME_PREFIX)
                    .unwrap()
                    .to_string()
            } else {
                self.name().to_string()
            },
            static_fields: self
                .static_fields()
                .map(|e| e.node(db))
                .collect::<Result<Vec<_>>>()?,
            inputs: match self.ast_type_block().input() {
                Some(input) => input
                    .args
                    .iter()
                    .map(|arg| {
                        let field_type = arg.1.field_type.repr(db)?;
                        Ok((arg.0.to_string(), field_type))
                    })
                    .collect::<Result<Vec<_>>>()?,
                None => Vec::new(),
            },
            docstring: self.get_documentation().map(Docstring),
        })
    }
}

impl Class {
    pub fn inputs(&self) -> &Vec<(String, FieldType)> {
        &self.inputs
    }
}

#[derive(Clone, Debug)]
pub struct TypeAlias {
    pub name: String,
    pub r#type: Node<FieldType>,
    pub docstring: Option<Docstring>,
}

impl WithRepr<TypeAlias> for TypeAliasWalker<'_> {
    fn attributes(&self, _: &ParserDatabase) -> NodeAttributes {
        NodeAttributes {
            span: Some(self.span().clone()),
            identifier_span: Some(self.identifier().span().clone()),
            ..Default::default() // TODO: Rest of attributes.
        }
    }

    fn repr(&self, db: &ParserDatabase) -> Result<TypeAlias> {
        Ok(TypeAlias {
            name: self.name().to_string(),
            r#type: self.target().node(db)?,
            docstring: None, // TODO: Type alias docstring
        })
    }
}

#[derive(serde::Serialize, Debug)]
pub enum OracleType {
    LLM,
}
#[derive(Debug)]
pub struct AliasOverride {
    pub name: String,
    // This is used to generate deserializers with aliased keys (see .overload in python deserializer)
    pub aliased_keys: Vec<AliasedKey>,
}

// TODO, also add skips
#[derive(Debug)]
pub struct AliasedKey {
    pub key: String,
    pub alias: UnresolvedValue<()>,
}

type ImplementationId = String;

#[derive(Debug)]
pub struct Implementation {
    r#type: OracleType,
    pub name: ImplementationId,
    pub function_name: String,

    pub prompt: Prompt,

    pub input_replacers: IndexMap<String, String>,

    pub output_replacers: IndexMap<String, String>,

    pub client: ClientId,

    /// Inputs for deserializer.overload in the generated code.
    ///
    /// This is NOT 1:1 with "override" clauses in the .baml file.
    ///
    /// For enums, we generate one for "alias", one for "description", and one for "alias: description"
    /// (this means that we currently don't support deserializing "alias[^a-zA-Z0-9]{1,5}description" but
    /// for now it suffices)
    pub overrides: Vec<AliasOverride>,
}

/// BAML does not allow UnnamedArgList nor a lone NamedArg
#[derive(serde::Serialize, Debug)]
pub enum FunctionArgs {
    UnnamedArg(FieldType),
    NamedArgList(Vec<(String, FieldType)>),
}

type FunctionId = String;

impl Function {
    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn output(&self) -> &FieldType {
        &self.output
    }

    pub fn inputs(&self) -> &Vec<(String, FieldType)> {
        &self.inputs
    }

    pub fn tests(&self) -> &Vec<Node<TestCase>> {
        &self.tests
    }

    pub fn configs(&self) -> Option<&Vec<FunctionConfig>> {
        Some(&self.configs)
    }
}

#[derive(Debug)]
pub struct Function {
    pub name: FunctionId,
    pub inputs: Vec<(String, FieldType)>,
    pub output: FieldType,
    pub tests: Vec<Node<TestCase>>,
    pub configs: Vec<FunctionConfig>,
    pub default_config: String,
}

#[derive(Debug)]
pub struct FunctionConfig {
    pub name: String,
    pub prompt_template: String,
    pub prompt_span: ast::Span,
    pub client: ClientSpec,
}

#[derive(Clone, Debug)]
pub struct ExprFunction {
    pub name: FunctionId,
    pub inputs: Vec<(String, FieldType)>,
    pub output: FieldType,
    pub expr: Expr<ExprMetadata>,
    pub tests: Vec<Node<TestCase>>,
}

impl ExprFunction {
    /// This is a temporary workaround for making expr_fns behave like llm_functions
    /// for the purpose of listing functions and tests in the playground.
    /// TODO: (Greg) handle different types of functions through separate paths.
    pub fn pretend_to_be_llm_function(&self) -> Function {
        Function {
            name: self.name.clone(),
            inputs: self.inputs.clone(),
            output: self.output.clone(),
            tests: self.tests.clone(),
            configs: vec![],
            default_config: "default_config".to_string(),
        }
    }

    pub fn inputs(&self) -> &Vec<(String, FieldType)> {
        &self.inputs
    }

    pub fn tests(&self) -> &Vec<Node<TestCase>> {
        &self.tests
    }

    /// Traverse the function body adding type annotations to variables that
    /// correspond to function parameters.
    pub fn assign_param_types_to_body_variables(self) -> Self {
        let new_expr = match &self.expr {
            Expr::Lambda(arity, body, meta) => {
                let body = Arc::unwrap_or_clone(body.clone());
                let new_body =
                    self.inputs
                        .iter()
                        .enumerate()
                        .fold(body, |body, (ind, (name, r#type))| {
                            let target = VarIndex {
                                de_bruijn: 0,
                                tuple: ind as u32,
                            };
                            annotate_variable(target, r#type.clone(), body)
                        });
                let res = Expr::Lambda(*arity, Arc::new(new_body), meta.clone());
                eprintln!(
                    "ASSIGN_PARAM_TYPES_TO_BODY_VARIABLES input:\n{:?}\nresult:\n{:?}",
                    self.expr, res
                );
                res
            }
            // TODO: Handle other cases - traverse the tree.
            // It seems like only Expr::Lambda is admissable as an ExprBody's expr field?
            _ => self.expr,
        };
        ExprFunction {
            expr: new_expr,
            ..self
        }
    }
}

/// For all variables under an expression, assign them the given type.
pub fn annotate_variable(
    target: VarIndex,
    r#type: FieldType,
    expr: Expr<ExprMetadata>,
) -> Expr<ExprMetadata> {
    match &expr {
        Expr::FreeVar(var_name, meta) => expr,
        Expr::BoundVar(var_index, meta) => {
            if var_index == &target {
                Expr::BoundVar(var_index.clone(), (meta.0.clone(), Some(r#type.clone())))
            } else {
                expr
            }
        }
        Expr::Lambda(arity, body, meta) => {
            let new_body = annotate_variable(
                target.deeper(),
                r#type.clone(),
                Arc::unwrap_or_clone(body.clone()),
            );
            Expr::Lambda(*arity, Arc::new(new_body), meta.clone())
        }
        Expr::App(f, args, meta) => {
            let new_f = annotate_variable(
                target.clone(),
                r#type.clone(),
                Arc::unwrap_or_clone(f.clone()),
            );
            let new_args =
                annotate_variable(target.clone(), r#type, Arc::unwrap_or_clone(args.clone()));
            Expr::App(Arc::new(new_f), Arc::new(new_args), meta.clone())
        }
        Expr::Let(var_name, expr, body, meta) => {
            let new_binding = annotate_variable(
                target.clone(),
                r#type.clone(),
                Arc::unwrap_or_clone(expr.clone()),
            );
            let new_body = Arc::new(annotate_variable(
                target.clone(),
                r#type.clone(),
                Arc::unwrap_or_clone(body.clone()),
            ));
            Expr::Let(
                var_name.clone(),
                Arc::new(new_binding),
                new_body,
                meta.clone(),
            )
        }
        Expr::ArgsTuple(args, meta) => Expr::ArgsTuple(
            args.iter()
                .map(|arg| annotate_variable(target.clone(), r#type.clone(), arg.clone()))
                .collect(),
            meta.clone(),
        ),
        Expr::Atom(_) => expr,
        Expr::LLMFunction(_, _, _) => expr,
        Expr::ClassConstructor {
            name,
            fields,
            spread,
            meta,
        } => {
            let new_fields = fields
                .iter()
                .map(|(key, value)| {
                    (
                        key.clone(),
                        annotate_variable(target.clone(), r#type.clone(), value.clone()),
                    )
                })
                .collect();
            let new_spread = match spread {
                None => None,
                Some(expr) => Some(Box::new(annotate_variable(
                    target,
                    r#type.clone(),
                    expr.as_ref().clone(),
                ))),
            };
            Expr::ClassConstructor {
                name: name.clone(),
                fields: new_fields,
                spread: new_spread,
                meta: meta.clone(),
            }
        }
        Expr::Map(entries, meta) => {
            let new_entries = entries
                .iter()
                .map(|(key, value)| {
                    (
                        key.clone(),
                        annotate_variable(target.clone(), r#type.clone(), value.clone()),
                    )
                })
                .collect();
            Expr::Map(new_entries, meta.clone())
        }
        Expr::List(items, meta) => {
            let new_items = items
                .iter()
                .map(|item| annotate_variable(target.clone(), r#type.clone(), item.clone()))
                .collect();
            Expr::List(new_items, meta.clone())
        }
    }
}

fn process_field(
    overrides: &IndexMap<(String, String), IndexMap<String, UnresolvedValue<()>>>, // Adjust the type according to your actual field type
    original_name: &str,
    function_name: &str,
    impl_name: &str,
) -> Vec<AliasedKey> {
    // This feeds into deserializer.overload; the registerEnumDeserializer counterpart is in generate_ts_client.rs
    match overrides.get(&((*function_name).to_string(), (*impl_name).to_string())) {
        Some(overrides) => {
            if let Some(UnresolvedValue::String(alias, ..)) = overrides.get("alias") {
                if let Some(UnresolvedValue::String(description, ..)) = overrides.get("description")
                {
                    // "alias" and "alias: description"
                    vec![
                        AliasedKey {
                            key: original_name.to_string(),
                            alias: UnresolvedValue::String(alias.clone(), ()),
                        },
                        // AliasedKey {
                        //     key: original_name.to_string(),
                        //     alias: UnresolvedValue::String(format!("{}: {}", alias, description)),
                        // },
                    ]
                } else {
                    // "alias"
                    vec![AliasedKey {
                        key: original_name.to_string(),
                        alias: UnresolvedValue::String(alias.clone(), ()),
                    }]
                }
            } else if let Some(UnresolvedValue::String(description, ..)) =
                overrides.get("description")
            {
                // "description"
                vec![AliasedKey {
                    key: original_name.to_string(),
                    alias: UnresolvedValue::String(description.clone(), ()),
                }]
            } else {
                // no overrides
                vec![]
            }
        }
        None => Vec::new(),
    }
}

impl WithRepr<Function> for FunctionWalker<'_> {
    fn attributes(&self, db: &ParserDatabase) -> NodeAttributes {
        let mut symbol_spans = HashMap::new();

        for arg in self.walk_input_args().chain(self.walk_output_args()) {
            let node_attrs = WithRepr::attributes(arg.field_type(), db);
            for (symbol, mut spans) in node_attrs.symbol_spans {
                if !symbol_spans.contains_key(&symbol) {
                    symbol_spans.insert(symbol, spans);
                } else {
                    symbol_spans.get_mut(&symbol).unwrap().append(&mut spans);
                }
            }
        }

        NodeAttributes {
            meta: Default::default(),
            constraints: Vec::new(),
            span: Some(self.span().clone()),
            identifier_span: Some(self.identifier().span().clone()),
            symbol_spans,
        }
    }

    fn repr(&self, db: &ParserDatabase) -> Result<Function> {
        Ok(Function {
            name: self.name().to_string(),
            inputs: self
                .ast_function()
                .input()
                .expect("msg")
                .args
                .iter()
                .map(|arg| {
                    let field_type = arg.1.field_type.repr(db)?;
                    Ok((arg.0.to_string(), field_type))
                })
                .collect::<Result<Vec<_>>>()?,
            output: self
                .ast_function()
                .output()
                .expect("need block arg")
                .field_type
                .repr(db)?,
            configs: vec![FunctionConfig {
                name: "default_config".to_string(),
                prompt_template: self.jinja_prompt().to_string(),
                prompt_span: self.ast_function().span().clone(),
                client: match self.client_spec() {
                    Ok(spec) => spec,
                    Err(e) => anyhow::bail!("{}", e.message()),
                },
            }],
            default_config: "default_config".to_string(),
            tests: self
                .walk_tests()
                .map(|e| e.node(db))
                .collect::<Result<Vec<_>>>()?,
        })
    }
}

type ClientId = String;

#[derive(Debug)]
pub struct Client {
    pub name: ClientId,
    pub provider: ClientProvider,
    pub retry_policy_id: Option<String>,
    pub options: UnresolvedClientProperty<()>,
}

impl WithRepr<Client> for ClientWalker<'_> {
    fn attributes(&self, _: &ParserDatabase) -> NodeAttributes {
        NodeAttributes {
            meta: IndexMap::new(),
            constraints: Vec::new(),
            span: Some(self.span().clone()),
            identifier_span: Some(self.identifier().span().clone()),
            symbol_spans: HashMap::new(),
        }
    }

    fn repr(&self, db: &ParserDatabase) -> Result<Client> {
        Ok(Client {
            name: self.name().to_string(),
            provider: self.properties().provider.0.clone(),
            options: self.properties().options.without_meta(),
            retry_policy_id: self
                .properties()
                .retry_policy
                .as_ref()
                .map(|(id, _)| id.clone()),
        })
    }
}

#[derive(serde::Serialize, Debug)]
pub struct RetryPolicyId(pub String);

#[derive(Debug)]
pub struct RetryPolicy {
    pub name: RetryPolicyId,
    pub max_retries: u32,
    pub strategy: RetryPolicyStrategy,
    // NB: the parser DB has a notion of "empty options" vs "no options"; we collapse
    // those here into an empty vec
    options: Vec<(String, UnresolvedValue<()>)>,
}

impl WithRepr<RetryPolicy> for ConfigurationWalker<'_> {
    fn attributes(&self, _db: &ParserDatabase) -> NodeAttributes {
        NodeAttributes {
            meta: IndexMap::new(),
            constraints: Vec::new(),
            span: Some(self.span().clone()),
            identifier_span: Some(self.identifier().span().clone()),
            symbol_spans: HashMap::new(),
        }
    }

    fn repr(&self, db: &ParserDatabase) -> Result<RetryPolicy> {
        Ok(RetryPolicy {
            name: RetryPolicyId(self.name().to_string()),
            max_retries: self.retry_policy().max_retries,
            strategy: self.retry_policy().strategy,
            options: match &self.retry_policy().options {
                Some(o) => o
                    .iter()
                    .map(|(k, (_, v))| Ok((k.clone(), v.without_meta())))
                    .collect::<Result<Vec<_>>>()?,
                None => vec![],
            },
        })
    }
}

// TODO: #1343 Temporary solution until we implement scoping in the AST.
#[derive(Clone, Debug)]
pub enum TypeBuilderEntry {
    Enum(Node<Enum>),
    Class(Node<Class>),
    TypeAlias(Node<TypeAlias>),
}

// TODO: #1343 Temporary solution until we implement scoping in the AST.
#[derive(Clone, Debug)]
pub struct TestTypeBuilder {
    pub entries: Vec<TypeBuilderEntry>,
    pub recursive_classes: Vec<IndexSet<String>>,
    pub recursive_aliases: Vec<IndexMap<String, FieldType>>,
}

#[derive(Clone, serde::Serialize, Debug)]
pub struct TestCaseFunction(String);

impl TestCaseFunction {
    pub fn name(&self) -> &str {
        &self.0
    }
}

#[derive(Clone, Debug)]
pub struct TestCase {
    pub name: String,
    pub functions: Vec<Node<TestCaseFunction>>,
    pub args: IndexMap<String, UnresolvedValue<()>>,
    pub constraints: Vec<Constraint>,
    pub type_builder: TestTypeBuilder,
}

impl WithRepr<TestCaseFunction> for (&ConfigurationWalker<'_>, usize) {
    fn attributes(&self, _db: &ParserDatabase) -> NodeAttributes {
        let span = self.0.test_case().functions[self.1].1.clone();
        let constraints = self
            .0
            .test_case()
            .constraints
            .iter()
            .map(|(c, _, _)| c)
            .cloned()
            .collect();
        NodeAttributes {
            meta: IndexMap::new(),
            constraints,
            span: Some(span.clone()),
            identifier_span: Some(span),
            symbol_spans: HashMap::new(),
        }
    }

    fn repr(&self, _db: &ParserDatabase) -> Result<TestCaseFunction> {
        Ok(TestCaseFunction(
            self.0.test_case().functions[self.1].0.clone(),
        ))
    }
}

impl WithRepr<TestCase> for ConfigurationWalker<'_> {
    fn attributes(&self, _db: &ParserDatabase) -> NodeAttributes {
        let constraints = self
            .test_case()
            .constraints
            .iter()
            .map(|(c, _, _)| c)
            .cloned()
            .collect();
        NodeAttributes {
            meta: IndexMap::new(),
            constraints,
            span: Some(self.span().clone()),
            identifier_span: Some(self.identifier().span().clone()),
            symbol_spans: HashMap::new(),
        }
    }

    fn repr(&self, db: &ParserDatabase) -> Result<TestCase> {
        let functions = (0..self.test_case().functions.len())
            .map(|i| (self, i).node(db))
            .collect::<Result<Vec<_>>>()?;

        // TODO: #1343 Temporary solution until we implement scoping in the AST.
        let (classes, enums, type_aliases, recursive_classes, recursive_aliases) =
            IntermediateRepr::type_builder_entries_from_scoped_db(
                &self.test_case().type_builder_scoped_db,
                db,
            )?;

        let mut type_builder_entries = Vec::new();

        for e in enums {
            type_builder_entries.push(TypeBuilderEntry::Enum(e));
        }
        for c in classes {
            type_builder_entries.push(TypeBuilderEntry::Class(c));
        }
        for a in type_aliases {
            type_builder_entries.push(TypeBuilderEntry::TypeAlias(a));
        }

        Ok(TestCase {
            name: self.name().to_string(),
            args: self
                .test_case()
                .args
                .iter()
                .map(|(k, (_, v))| Ok((k.clone(), v.without_meta())))
                .collect::<Result<IndexMap<_, _>>>()?,
            functions,
            constraints: <AstWalker<'_, (ValExpId, &str)> as WithRepr<TestCase>>::attributes(
                self, db,
            )
            .constraints
            .into_iter()
            .collect::<Vec<_>>(),
            type_builder: TestTypeBuilder {
                entries: type_builder_entries,
                recursive_aliases,
                recursive_classes,
            },
        })
    }
}

#[derive(Debug, Clone, Serialize)]
pub enum Prompt {
    // The prompt stirng, and a list of input replacer keys (raw key w/ magic string, and key to replace with)
    String(String, Vec<(String, String)>),

    // same thing, the chat message, and the replacer input keys (raw key w/ magic string, and key to replace with)
    Chat(Vec<ChatMessage>, Vec<(String, String)>),
}

#[derive(serde::Serialize, Debug, Clone)]
pub struct ChatMessage {
    pub idx: u32,
    pub role: String,
    pub content: String,
}

impl WithRepr<Prompt> for PromptAst<'_> {
    fn repr(&self, _db: &ParserDatabase) -> Result<Prompt> {
        Ok(match self {
            PromptAst::String(content, _) => Prompt::String(content.clone(), vec![]),
            PromptAst::Chat(messages, input_replacers) => Prompt::Chat(
                messages
                    .iter()
                    .filter_map(|(message, content)| {
                        message.as_ref().map(|m| ChatMessage {
                            idx: m.idx,
                            role: m.role.0.clone(),
                            content: content.clone(),
                        })
                    })
                    .collect::<Vec<_>>(),
                input_replacers.to_vec(),
            ),
        })
    }
}

/// Generate an IntermediateRepr from a single block of BAML source code.
/// This is useful for generating IR test fixtures.
pub fn make_test_ir(source_code: &str) -> anyhow::Result<IntermediateRepr> {
    let (ir, diagnostics) = make_test_ir_and_diagnostics(source_code)?;
    if diagnostics.has_errors() {
        return Err(anyhow::anyhow!(
            "Source code was invalid: \n{:?}",
            diagnostics.errors()
        ));
    } else {
        Ok(ir)
    }
}

/// Generate an IntermediateRepr from a single block of BAML source code.
/// This is useful for generating IR test fixtures. Also return the
/// `Diagnostics`.
pub fn make_test_ir_and_diagnostics(
    source_code: &str,
) -> anyhow::Result<(IntermediateRepr, Diagnostics)> {
    use crate::validate;
    use crate::ValidatedSchema;
    use internal_baml_diagnostics::SourceFile;
    use std::path::PathBuf;

    let path: PathBuf = "fake_file.baml".into();
    let source_file: SourceFile = (path.clone(), source_code).into();
    let validated_schema: ValidatedSchema = validate(&path, vec![source_file]);
    let diagnostics = validated_schema.diagnostics;
    let ir = IntermediateRepr::from_parser_database(
        &validated_schema.db,
        validated_schema.configuration,
    )?;
    Ok((ir, diagnostics))
}

/// Pull out `StreamingBehavior` from `NodeAttributes`.
fn streaming_behavior_from_attributes(attributes: &NodeAttributes) -> StreamingBehavior {
    fn is_some_true(maybe_value: Option<&UnresolvedValue<()>>) -> bool {
        match maybe_value {
            Some(Resolvable::Bool(true, _)) => true,
            _ => false,
        }
    }
    StreamingBehavior {
        done: is_some_true(attributes.get("stream.done")),
        state: is_some_true(attributes.get("stream.with_state")),
    }
}

/// Create a context from the expr_functions, top_level_assignments, and
/// functions in the IR.
/// This context is used in evaluating expressions.
pub fn initial_context(ir: &IntermediateRepr) -> HashMap<Name, Expr<ExprMetadata>> {
    let mut ctx = HashMap::new();

    for expr_fn in ir.expr_fns.iter() {
        ctx.insert(expr_fn.elem.name.clone(), expr_fn.elem.expr.clone());
    }
    for top_level_assignment in ir.toplevel_assignments.iter() {
        ctx.insert(
            top_level_assignment.elem.name.elem.clone(),
            top_level_assignment.elem.expr.elem.clone(),
        );
    }
    for llm_function in ir.functions.iter() {
        let params = llm_function
            .elem
            .inputs
            .iter()
            .map(|arg| arg.0.clone())
            .collect::<Vec<_>>();
        let params_type: Vec<FieldType> = llm_function
            .elem
            .inputs
            .iter()
            .map(|arg| arg.1.clone())
            .collect::<Vec<_>>();
        let body_type = llm_function.elem.output.clone();
        let lambda_type = FieldType::Arrow(Box::new(Arrow {
            param_types: params_type,
            return_type: body_type,
        }));
        ctx.insert(
            llm_function.elem.name.clone(),
            Expr::LLMFunction(
                llm_function.elem.name.clone(),
                params,
                (
                    llm_function
                        .attributes
                        .span
                        .as_ref()
                        .expect("LLM Functions have spans until we use dynamic types")
                        .clone(),
                    Some(lambda_type),
                ),
            ),
        );
    }
    ctx
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ir::{ir_helpers::IRHelper, TypeValue};

    #[test]
    fn test_docstrings() {
        let ir = make_test_ir(
            r#"
          /// Foo class.
          class Foo {
            /// Bar field.
            bar string

            /// Baz field.
            baz int
          }

          /// Test enum.
          enum TestEnum {
            /// First variant.
            FIRST

            /// Second variant.
            SECOND

            THIRD
          }
        "#,
        )
        .unwrap();

        // Test class docstrings
        let foo = ir.find_class("Foo").as_ref().unwrap().clone().elem();
        assert_eq!(foo.docstring.as_ref().unwrap().0.as_str(), "Foo class.");
        match foo.static_fields.as_slice() {
            [field1, field2] => {
                assert_eq!(field1.elem.docstring.as_ref().unwrap().0, "Bar field.");
                assert_eq!(field2.elem.docstring.as_ref().unwrap().0, "Baz field.");
            }
            _ => {
                panic!("Expected 2 fields");
            }
        }

        // Test enum docstrings
        let test_enum = ir.find_enum("TestEnum").as_ref().unwrap().clone().elem();
        assert_eq!(
            test_enum.docstring.as_ref().unwrap().0.as_str(),
            "Test enum."
        );
        match test_enum.values.as_slice() {
            [val1, val2, val3] => {
                assert_eq!(val1.0.elem.0, "FIRST");
                assert_eq!(val1.1.as_ref().unwrap().0, "First variant.");
                assert_eq!(val2.0.elem.0, "SECOND");
                assert_eq!(val2.1.as_ref().unwrap().0, "Second variant.");
                assert_eq!(val3.0.elem.0, "THIRD");
                assert!(val3.1.is_none());
            }
            _ => {
                panic!("Expected 3 enum values");
            }
        }
    }

    #[test]
    fn test_block_attributes() {
        let ir = make_test_ir(
            r##"
            client<llm> GPT4 {
              provider openai
              options {
                model gpt-4o
                api_key env.OPENAI_API_KEY
              }
            }
            function Foo(a: int) -> int {
              client GPT4
              prompt #"Double the number {{ a }}"#
            }

            test Foo() {
              functions [Foo]
              args {
                a 10
              }
              @@assert( {{ result == 20 }} )
            }
        "##,
        )
        .unwrap();
        let function = ir.find_function("Foo").unwrap();
        let walker = ir.find_test(&function, "Foo").unwrap();
        assert_eq!(walker.item.1.elem.constraints.len(), 1);
    }

    #[test]
    fn test_streaming_attributes() {
        let ir = make_test_ir(
            r##"
            class Foo {
              foo_int int @stream.not_null
              foo_bool bool @stream.with_state
              foo_list int[] @stream.done
            }

            class Bar {
              name string @stream.done
              message string
              @@stream.done
            }
        "##,
        )
        .unwrap();
        let foo = ir.find_class("Foo").unwrap();
        assert!(!foo.streaming_done());
        match foo.walk_fields().collect::<Vec<_>>().as_slice() {
            [field1, field2, field3] => {
                let type1 = &field1.item.elem.r#type;
                assert!(field1.streaming_needed());
                assert!(type1.attributes.get("stream.not_null").is_none());
                let type2 = &field2.item.elem.r#type;
                assert!(!field2.streaming_state());
                assert!(type2.attributes.get("stream.with_state").is_some());
                let type3 = &field3.item.elem.r#type;
                assert!(!field3.streaming_done());
                assert!(type3.attributes.get("stream.done").is_some());
            }
            _ => panic!("Expected exactly 3 fields"),
        }
        let bar = ir.find_class("Bar").unwrap();
        assert!(bar.streaming_done());
        match bar.walk_fields().collect::<Vec<_>>().as_slice() {
            [field1, field2] => {
                assert!(!field1.streaming_done());
                assert!(field1
                    .item
                    .elem
                    .r#type
                    .attributes
                    .get("stream.done")
                    .is_some());
            }
            _ => panic!("Expected exactly 2 fields"),
        }
    }

    fn test_resolve_type_alias() {
        let ir = make_test_ir(
            r##"
            type One = int
            type Two = One
            type Three = Two

            class Test {
                field Three
            }
        "##,
        )
        .unwrap();

        let class = ir.find_class("Test").unwrap();
        let alias = class.find_field("field").unwrap();

        assert_eq!(*alias.r#type(), FieldType::Primitive(TypeValue::Int));
    }

    #[test]
    fn test_merge_type_alias_attributes() {
        let ir = make_test_ir(
            r##"
            type One = int @check(gt_ten, {{ this > 10 }})
            type Two = One @check(lt_twenty, {{ this < 20 }})
            type Three = Two @assert({{ this != 15 }})

            class Test {
                field Three
            }
        "##,
        )
        .unwrap();

        let class = ir.find_class("Test").unwrap();
        let alias = class.find_field("field").unwrap();

        let FieldType::WithMetadata {
            base, constraints, ..
        } = alias.r#type()
        else {
            panic!(
                "expected resolved constrained type, found {:?}",
                alias.r#type()
            );
        };

        assert_eq!(constraints.len(), 3);

        assert_eq!(constraints[0].level, ConstraintLevel::Assert);
        assert_eq!(constraints[0].label, None);

        assert_eq!(constraints[1].level, ConstraintLevel::Check);
        assert_eq!(constraints[1].label, Some("lt_twenty".to_string()));

        assert_eq!(constraints[2].level, ConstraintLevel::Check);
        assert_eq!(constraints[2].label, Some("gt_ten".to_string()));
    }

    #[test]
    fn test_expr_fn_tests() {
        let ir = make_test_ir(
            r##"
            fn Foo(x: int) -> int {
                x
            }

            test FooTest {
                functions [Foo]
                args {
                    x 1
                }
            }
        "##,
        )
        .unwrap();

        let function = ir.find_expr_fn("Foo").unwrap();
        let test = ir.find_expr_fn_test(&function, "FooTest").unwrap();
        assert_eq!(test.item.1.elem.functions.len(), 1);
        assert_eq!(test.item.1.elem.functions[0].elem.name(), "Foo");
    }
}
