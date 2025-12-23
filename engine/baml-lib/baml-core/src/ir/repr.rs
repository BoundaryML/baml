use std::{
    collections::{HashMap, HashSet},
    sync::Arc,
};

use anyhow::{anyhow, Result};
use baml_types::{
    baml_value::TypeLookups,
    expr::{self, Builtin, Expr, ExprMetadata, Name, VarIndex},
    ir_type::{ArrowGeneric, TypeNonStreaming, TypeStreaming, UnionConstructor},
    type_meta, Arrow, BamlMap, BamlValueWithMeta, Constraint, ConstraintLevel, JinjaExpression,
    Resolvable, StreamingMode, StringOr, TypeIR, TypeValue, UnionType, UnresolvedValue,
};
use either::Either;
use indexmap::{IndexMap, IndexSet};
use internal_baml_ast::ast::{
    self, Attribute, FieldArity, SubType, ValExpId, WithAttributes, WithIdentifier, WithName,
    WithSpan,
};
use internal_baml_diagnostics::{DatamodelWarning, Diagnostics, Span};
use internal_baml_parser_database::{
    walkers::{
        ClassWalker, ClientWalker, ConfigurationWalker, EnumValueWalker, EnumWalker, ExprFnWalker,
        FieldWalker, FunctionWalker, TemplateStringWalker, TopLevelAssignmentWalker,
        TypeAliasWalker, Walker as AstWalker,
    },
    Attributes, ParserDatabase, PromptAst, RetryPolicyStrategy, TypeWalker,
};
use internal_llm_client::{ClientProvider, ClientSpec, UnresolvedClientProperty};
use serde::Serialize;

use super::builtin::{builtin_classes, builtin_generic_fn, builtin_ir, is_builtin_identifier};
use crate::Configuration;

/// This class represents the intermediate representation of the BAML AST.
/// It is a representation of the BAML AST that is easier to work with than the
/// raw BAML AST, and should include all information necessary to generate
/// code in any target language.
#[derive(Debug)]
pub struct IntermediateRepr {
    pub enums: Vec<Node<Enum>>,
    pub classes: Vec<Node<Class>>,
    pub type_aliases: Vec<Node<TypeAlias>>,
    pub functions: Vec<Node<Function>>,
    pub expr_fns: Vec<Node<ExprFunction>>,
    pub toplevel_assignments: Vec<Node<TopLevelAssignment>>,
    pub clients: Vec<Node<Client>>,
    pub retry_policies: Vec<Node<RetryPolicy>>,
    pub template_strings: Vec<Node<TemplateString>>,

    /// Strongly connected components of the dependency graph (finite cycles).
    pub finite_recursive_cycles: Vec<IndexSet<String>>,

    /// Type alias cycles introduced by lists and maps and unions.
    ///
    /// These are the only allowed cycles, because lists and maps introduce a
    /// level of indirection that makes the cycle finite.
    pub structural_recursive_alias_cycles: Vec<IndexMap<String, TypeIR>>,

    pub configuration: Configuration,

    // only constructed after the first pass
    pub pass2_repr: Pass2Repr,
}

#[derive(Default, Debug)]
pub struct Pass2Repr {
    classes_with_attributes: BamlMap<String, NodeAttributes>,
    enums_with_attributes: BamlMap<String, NodeAttributes>,
    resolved_type_aliases: BamlMap<String, TypeIR>,
}

impl Pass2Repr {
    fn update_type(&self, type_generic: &mut TypeIR) {
        use baml_types::ir_type::TypeGeneric;
        match type_generic {
            TypeGeneric::Enum {
                name,
                dynamic,
                meta,
            } => {
                if let Some(attributes) = self.enums_with_attributes.get(name) {
                    *dynamic |= attributes.dynamic();
                    meta.streaming_behavior = meta
                        .streaming_behavior
                        .combine(&attributes.streaming_behavior());
                    meta.streaming_behavior.done = true;
                    meta.constraints.extend(attributes.constraints.clone());
                }
            }
            TypeGeneric::Class {
                name,
                mode,
                dynamic,
                meta,
            } => {
                if let Some(attributes) = self.classes_with_attributes.get(name) {
                    *dynamic |= attributes.dynamic();
                    meta.streaming_behavior = meta
                        .streaming_behavior
                        .combine(&attributes.streaming_behavior());
                    meta.constraints.extend(attributes.constraints.clone());
                }
            }
            TypeGeneric::Primitive(TypeValue::Int | TypeValue::Float | TypeValue::Bool, meta)
            | TypeGeneric::Literal(.., meta) => {
                meta.streaming_behavior.done = true;
            }
            TypeGeneric::Top(_)
            | TypeGeneric::Primitive(
                TypeValue::String | TypeValue::Media(..) | TypeValue::Null,
                ..,
            )
            | TypeGeneric::RecursiveTypeAlias { .. } => {}
            TypeGeneric::List(element, meta) => {
                meta.streaming_behavior.needed = true;
                element.meta_mut().streaming_behavior.needed = true;
                self.update_type(element);
            }
            TypeGeneric::Map(key, value, meta) => {
                meta.streaming_behavior.needed = true;
                key.meta_mut().streaming_behavior.needed = true;
                value.meta_mut().streaming_behavior.needed = true;
                self.update_type(key);
                self.update_type(value);
            }
            TypeGeneric::Tuple(type_generics, _) => {
                type_generics.iter_mut().for_each(|t| self.update_type(t))
            }
            TypeGeneric::Arrow(arrow_generic, _) => {
                self.update_type(&mut arrow_generic.return_type)
            }
            TypeGeneric::Union(union_type_generic, _) => union_type_generic
                .iter_skip_null_mut()
                .iter_mut()
                .for_each(|t| self.update_type(t)),
        }
    }
}

impl TypeLookups for IntermediateRepr {
    fn expand_recursive_type(&self, name: &str) -> anyhow::Result<&TypeIR> {
        match self.pass2_repr.resolved_type_aliases.get(name) {
            Some(ty) => Ok(ty),
            None => anyhow::bail!("Type alias not found: {name}"),
        }
    }
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
        let expr = self.top_level_assignment().stmt.expr.repr(db)?;
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
    fn attributes(&self, _db: &ParserDatabase) -> NodeAttributes {
        NodeAttributes {
            meta: Default::default(),
            constraints: Vec::new(),
            span: Some(self.expr_fn().span.clone()),
            identifier_span: Some(self.expr_fn().name.span().clone()),
            symbol_spans: HashMap::new(),
        }
    }

    fn repr(&self, db: &ParserDatabase) -> Result<ExprFunction> {
        let body = convert_function_body(self.expr_fn().body.to_owned(), db)?;
        let args: Vec<(String, TypeIR)> = self
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
        let lambda_type = TypeIR::Arrow(
            Box::new(ArrowGeneric {
                param_types: arg_types,
                return_type: return_type.clone(),
            }),
            Default::default(),
        );
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

impl WithRepr<Function> for ExprFnWalker<'_> {
    fn repr(&self, db: &ParserDatabase) -> Result<Function> {
        let body = convert_function_body(self.expr_fn().body.to_owned(), db)?;
        let args = self
            .expr_fn()
            .args
            .args
            .iter()
            .map(|(arg_name, arg_type)| Ok((arg_name.to_string(), arg_type.field_type.repr(db)?)))
            .collect::<Result<_>>()?;
        let return_type = self
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
            output: return_type,
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
    function_body
        .expr
        .map(|e| e.repr(db))
        .unwrap_or_else(|| {
            // eprintln!("TODO @greg: convert blocks with no return types to lambda terms");
            // Placeholder just to allow compilation.
            Ok(Expr::Atom(BamlValueWithMeta::Null((Span::fake(), None))))
        })
        .map(|fn_body| {
            let mut stmts = function_body.stmts.clone();
            stmts.reverse();
            let expr = stmts
                .iter()
                .filter(|stmt| matches!(stmt, ast::Stmt::Let(_))) // TODO: @greg
                .fold(fn_body, |acc, stmt| match stmt.body().repr(db) {
                    Ok(stmt_expr) => Expr::Let(
                        stmt.identifier().name().to_string(),
                        Arc::new(stmt_expr),
                        Arc::new(acc),
                        (stmt.span().clone(), None),
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
                (span.clone(), Some(TypeIR::bool())),
            ))),
            ast::Expression::NumericValue(val, span) => {
                // Prefer int when it parses cleanly; otherwise fall back to float.
                if let Ok(v) = val.parse::<i64>() {
                    Ok(Expr::Atom(BamlValueWithMeta::Int(
                        v,
                        (span.clone(), Some(TypeIR::int())),
                    )))
                } else if let Ok(f) = val.parse::<f64>() {
                    Ok(Expr::Atom(BamlValueWithMeta::Float(
                        f,
                        (span.clone(), Some(TypeIR::float())),
                    )))
                } else {
                    Err(anyhow!("Invalid numeric value: {}", val))
                }
            }
            ast::Expression::StringValue(val, span) => Ok(Expr::Atom(BamlValueWithMeta::String(
                val.to_string(),
                (span.clone(), Some(TypeIR::string())),
            ))),
            ast::Expression::RawStringValue(val) => Ok(Expr::Atom(BamlValueWithMeta::String(
                val.value().to_string(),
                (val.span().clone(), Some(TypeIR::string())),
            ))),
            ast::Expression::JinjaExpressionValue(val, span) => Ok(Expr::Atom(
                BamlValueWithMeta::String(val.to_string(), (span.clone(), Some(TypeIR::string()))),
            )),
            ast::Expression::Array(vals, span) => {
                let new_items = vals
                    .iter()
                    .map(|v| v.repr(db))
                    .collect::<Result<Vec<_>>>()?;
                let item_types = new_items
                    .iter()
                    .filter_map(|v| v.meta().1.clone())
                    .collect::<Vec<_>>();
                let list_type = match item_types.len() {
                    0 => None,
                    _ => Some(TypeIR::union(item_types).as_list()),
                };
                Ok(Expr::List(new_items, (span.clone(), list_type)))
            }
            ast::Expression::Map(vals, span) => {
                let new_items = vals
                    .iter()
                    .map(|(k, v)| v.repr(db).map(|v2| (k.to_string(), v2)))
                    .collect::<Result<IndexMap<_, _>>>()?;
                let item_types = new_items
                    .iter()
                    .filter_map(|v| v.1.meta().1.clone())
                    .collect::<Vec<_>>();

                let item_type = if item_types.is_empty() {
                    None
                } else {
                    Some(TypeIR::union(item_types))
                };

                // TODO: Is this correct?
                let key_type = TypeIR::string();
                let map_type = item_type.map(|t| TypeIR::map(key_type, t));
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
            ast::Expression::App(app) => {
                // Mangle names.
                //
                // TODO: Should probably be a separate pass on the IR similar
                // to fn specialize_generics, but there are some issues with
                // Arc<> and &mut and stuff cause we need to either mutate the
                // IR in place or build a new one, so for now this thing can
                // live here.
                let name = if let Some(ty) = app.type_args.first() {
                    format!("{}<{}>", app.name, ty)
                } else {
                    app.name.to_string()
                };

                let func = Expr::FreeVar(name, (app.span().clone(), None));

                let args = app
                    .args
                    .iter()
                    .map(|arg| arg.repr(db))
                    .collect::<Result<_>>()?;
                Ok(Expr::App {
                    func: Arc::new(func),
                    // TODO: We don't really have a span for the ArgsTuple, so we're using the one for the whole FnApp.
                    args: Arc::new(Expr::ArgsTuple(args, (app.span().clone(), None))),
                    type_args: app
                        .type_args
                        .iter()
                        .map(|t| t.repr(db))
                        .collect::<Result<_>>()?,
                    meta: (app.span().clone(), None),
                })
            }
            ast::Expression::ClassConstructor(
                ast::ClassConstructor {
                    class_name, fields, ..
                },
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
                    meta: (span.clone(), Some(TypeIR::class(class_name.name()))),
                })
            }
            ast::Expression::ExprBlock(block, span) => {
                // We use "function_body" and "expr_block" interchangeably.
                // This may need to be revisited?
                let body = convert_function_body(block.clone(), db)?;
                Ok(body)
            }
            ast::Expression::If(cond, then, else_, span) => {
                let cond = cond.repr(db)?;
                let then = then.repr(db)?;
                let else_ = else_.as_ref().map(|e| e.repr(db)).transpose()?;
                Ok(Expr::If(
                    Arc::new(cond),
                    Arc::new(then),
                    else_.map(Arc::new),
                    (span.clone(), None),
                ))
            }
            ast::Expression::ArrayAccess(base, index, span) => {
                let base_ir = base.repr(db)?;
                let index_ir = index.repr(db)?;
                Ok(Expr::ArrayAccess {
                    base: Arc::new(base_ir),
                    index: Arc::new(index_ir),
                    meta: (span.clone(), None), // Type will be inferred later
                })
            }
            ast::Expression::FieldAccess(base, field, span) => {
                let base_ir = base.repr(db)?;
                Ok(Expr::FieldAccess {
                    base: Arc::new(base_ir),
                    field: field.name().to_string(),
                    meta: (span.clone(), None), // Type will be inferred later
                })
            }
            // TODO: impl this (needs to compile, can't panic).
            ast::Expression::MethodCall { span, .. } => {
                Ok(Expr::Atom(BamlValueWithMeta::Null((span.clone(), None))))
            }
            ast::Expression::BinaryOperation {
                left,
                operator,
                right,
                span,
            } => {
                let left_ir = left.repr(db)?;
                let right_ir = right.repr(db)?;
                Ok(Expr::BinaryOperation {
                    left: Arc::new(left_ir),
                    operator: match operator {
                        ast::BinaryOperator::Eq => expr::BinaryOperator::Eq,
                        ast::BinaryOperator::Neq => expr::BinaryOperator::Neq,
                        ast::BinaryOperator::Lt => expr::BinaryOperator::Lt,
                        ast::BinaryOperator::LtEq => expr::BinaryOperator::LtEq,
                        ast::BinaryOperator::Gt => expr::BinaryOperator::Gt,
                        ast::BinaryOperator::GtEq => expr::BinaryOperator::GtEq,
                        ast::BinaryOperator::Add => expr::BinaryOperator::Add,
                        ast::BinaryOperator::Sub => expr::BinaryOperator::Sub,
                        ast::BinaryOperator::Mul => expr::BinaryOperator::Mul,
                        ast::BinaryOperator::Div => expr::BinaryOperator::Div,
                        ast::BinaryOperator::Mod => expr::BinaryOperator::Mod,
                        ast::BinaryOperator::BitAnd => expr::BinaryOperator::BitAnd,
                        ast::BinaryOperator::BitOr => expr::BinaryOperator::BitOr,
                        ast::BinaryOperator::BitXor => expr::BinaryOperator::BitXor,
                        ast::BinaryOperator::Shl => expr::BinaryOperator::Shl,
                        ast::BinaryOperator::Shr => expr::BinaryOperator::Shr,
                        ast::BinaryOperator::And => expr::BinaryOperator::And,
                        ast::BinaryOperator::Or => expr::BinaryOperator::Or,
                        ast::BinaryOperator::InstanceOf => expr::BinaryOperator::InstanceOf,
                    },
                    right: Arc::new(right_ir),
                    meta: (span.clone(), None),
                })
            }
            ast::Expression::UnaryOperation {
                expr,
                operator,
                span,
            } => {
                let expr_ir = expr.repr(db)?;
                Ok(Expr::UnaryOperation {
                    expr: Arc::new(expr_ir),
                    operator: match operator {
                        ast::UnaryOperator::Not => expr::UnaryOperator::Not,
                        ast::UnaryOperator::Neg => expr::UnaryOperator::Neg,
                    },
                    meta: (span.clone(), None),
                })
            }
            // Don't care.
            ast::Expression::Paren(expr, span) => expr.repr(db),
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
            pass2_repr: Pass2Repr::default(),
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

        env_vars
    }

    /// Extend the IR with another IR.
    pub fn extend(&mut self, other: IntermediateRepr) {
        self.enums.extend(other.enums);
        self.classes.extend(other.classes);
        self.type_aliases.extend(other.type_aliases);
        self.functions.extend(other.functions);
        self.expr_fns.extend(other.expr_fns);
        self.toplevel_assignments.extend(other.toplevel_assignments);
        self.clients.extend(other.clients);
        self.retry_policies.extend(other.retry_policies);
        self.template_strings.extend(other.template_strings);
    }

    /// Returns a list of all the recursive cycles in the IR.
    ///
    /// Each cycle is represented as a set of strings, where each string is the
    /// name of a class.
    pub fn finite_recursive_cycles(&self) -> &[IndexSet<String>] {
        &self.finite_recursive_cycles
    }

    pub fn structural_recursive_alias_cycles(&self) -> &[IndexMap<String, TypeIR>] {
        &self.structural_recursive_alias_cycles
    }

    pub fn walk_enums(&self) -> impl ExactSizeIterator<Item = Walker<'_, &Node<Enum>>> {
        self.enums.iter().map(|e| Walker { ir: self, item: e })
    }

    pub fn walk_classes(&self) -> impl Iterator<Item = Walker<'_, &Node<Class>>> {
        self.classes.iter().map(|e| Walker { ir: self, item: e })
    }

    pub fn walk_type_aliases(&self) -> impl ExactSizeIterator<Item = Walker<'_, &Node<TypeAlias>>> {
        self.type_aliases
            .iter()
            .map(|e| Walker { ir: self, item: e })
    }

    // TODO: Exact size Iterator + Node<>?
    pub fn walk_alias_cycles(&self) -> impl Iterator<Item = Walker<'_, (&String, &TypeIR)>> {
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

    fn walk_all_types_with_filter<F, T>(&self, filter: F) -> Vec<T>
    where
        F: Fn(&TypeIR) -> Vec<T>,
    {
        let class_fields = self
            .classes
            .iter()
            .flat_map(|c| c.elem.static_fields.iter().map(|f| &f.elem.r#type.elem));

        let type_alias_fields = self.type_aliases.iter().map(|c| &c.elem.r#type.elem);

        let function_fields = self.functions.iter().flat_map(|f| {
            f.elem
                .inputs
                .iter()
                .map(|(_, t)| t)
                .chain(std::iter::once(&f.elem.output))
        });

        let all_types = class_fields.chain(type_alias_fields).chain(function_fields);

        all_types.flat_map(filter).collect::<Vec<_>>()
    }

    fn walk_all_non_streaming_types_with_filter<F>(&self, filter: F) -> Vec<TypeNonStreaming>
    where
        F: Fn(&TypeNonStreaming) -> bool,
    {
        let is_non_streaming = move |t: &TypeIR| -> Vec<TypeNonStreaming> {
            let t = t.to_non_streaming_type(self);
            t.find_if(&filter, false)
                .into_iter()
                .cloned()
                .collect::<Vec<_>>()
        };

        self.walk_all_types_with_filter(is_non_streaming)
    }

    fn walk_all_streaming_types_with_filter<F>(&self, filter: F) -> Vec<TypeStreaming>
    where
        F: Fn(&TypeStreaming) -> bool,
    {
        let is_streaming = move |t: &TypeIR| -> Vec<TypeStreaming> {
            let t = t.to_streaming_type(self);
            t.find_if(&filter, false)
                .into_iter()
                .cloned()
                .collect::<Vec<_>>()
        };

        self.walk_all_types_with_filter(is_streaming)
    }

    pub fn walk_all_types_with_checks(&self) -> impl Iterator<Item = TypeNonStreaming> {
        self.walk_all_non_streaming_types_with_filter(|t| {
            t.meta()
                .constraints
                .iter()
                .any(|c| c.level == ConstraintLevel::Check)
        })
        .into_iter()
    }

    pub fn walk_all_streaming_types_with_stream_state(
        &self,
    ) -> impl Iterator<Item = TypeStreaming> {
        self.walk_all_streaming_types_with_filter(|t| t.meta().streaming_behavior.state)
            .into_iter()
    }

    pub fn walk_all_non_streaming_unions(&self) -> impl Iterator<Item = TypeNonStreaming> {
        self.walk_all_non_streaming_types_with_filter(|t| matches!(t, TypeNonStreaming::Union(..)))
            .into_iter()
    }

    pub fn walk_all_streaming_types_with_checks(&self) -> impl Iterator<Item = TypeStreaming> {
        self.walk_all_streaming_types_with_filter(|t| {
            t.meta()
                .constraints
                .iter()
                .any(|c| c.level == ConstraintLevel::Check)
        })
        .into_iter()
    }

    pub fn walk_all_streaming_unions(&self) -> impl Iterator<Item = TypeStreaming> {
        self.walk_all_streaming_types_with_filter(|t| matches!(t, TypeStreaming::Union(..)))
            .into_iter()
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

    pub fn walk_function_test_pairs(
        &self,
    ) -> impl Iterator<Item = Walker<'_, (&Node<Function>, &Node<TestCase>)>> {
        self.functions.iter().flat_map(move |f| {
            f.elem.tests().iter().map(move |t| Walker {
                ir: self,
                item: (f, t),
            })
        })
    }

    pub fn walk_expr_fn_test_pairs(
        &self,
    ) -> impl Iterator<Item = Walker<'_, (&Node<ExprFunction>, &Node<TestCase>)>> {
        self.expr_fns.iter().flat_map(move |f| {
            f.elem.tests.iter().map(move |t| Walker {
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
                .map(|c| c.node(db))
                .collect::<Result<Vec<_>>>()?,
            type_aliases: db
                .walk_type_aliases()
                .map(|a| a.node(db))
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
            pass2_repr: Pass2Repr::default(),
        };

        // Sort each item by name.
        repr.enums.sort_by(|a, b| a.elem.name.cmp(&b.elem.name));
        repr.classes.sort_by(|a, b| a.elem.name.cmp(&b.elem.name));
        repr.functions
            .sort_by(|a, b| a.elem.name().cmp(b.elem.name()));
        repr.expr_fns.sort_by(|a, b| a.elem.name.cmp(&b.elem.name));
        repr.clients.sort_by(|a, b| a.elem.name.cmp(&b.elem.name));
        repr.retry_policies
            .sort_by(|a, b| a.elem.name.0.cmp(&b.elem.name.0));

        // Strip out builtin classes.
        repr.classes
            .retain(|c| !is_builtin_identifier(&c.elem.name));

        // all return types of functions must be set to needed
        for f in repr.functions.iter_mut() {
            f.elem.output.meta_mut().streaming_behavior.needed = true;
        }

        repr.distribute_attributes();

        Ok(repr)
    }

    fn set_pass2_repr(&mut self) {
        let default_streaming_behavior = type_meta::base::StreamingBehavior::default();
        let classes_with_attributes = self
            .classes
            .iter()
            .filter_map(|c| {
                if c.attributes.dynamic()
                    || c.attributes.streaming_behavior() != default_streaming_behavior
                    || !c.attributes.constraints.is_empty()
                {
                    Some((c.elem.name.clone(), c.attributes.clone()))
                } else {
                    None
                }
            })
            .collect::<BamlMap<_, _>>();
        let enums_with_attributes = self
            .enums
            .iter()
            .filter_map(|e| {
                if e.attributes.dynamic()
                    || e.attributes.streaming_behavior() != default_streaming_behavior
                    || !e.attributes.constraints.is_empty()
                {
                    Some((e.elem.name.clone(), e.attributes.clone()))
                } else {
                    None
                }
            })
            .collect::<BamlMap<_, _>>();

        self.pass2_repr.classes_with_attributes = classes_with_attributes;
        self.pass2_repr.enums_with_attributes = enums_with_attributes;
        self.pass2_repr.resolved_type_aliases = self
            .structural_recursive_alias_cycles
            .iter()
            .flat_map(|i| i.iter())
            .map(|(name, type_)| (name.clone(), type_.clone()))
            .collect();
    }

    /// Modifies the type to inject any block level attributes that are present on the class or enum.
    pub fn finalize_type(&self, type_generic: &mut TypeIR) {
        self.pass2_repr.update_type(type_generic);
    }

    // For each test, check that its arguments are valid - that they
    // have the correct name and type for the function under test.
    // If there are required args but the test has an empty args block,
    // Produce an error message with a fully example of an args block with
    // dummy args.
    // If there are some args in the test block, give examples of all the
    // missing args.
    pub fn validate_test_args(&self, diagnostics: &mut Diagnostics) {
        use std::collections::HashSet;

        use crate::ir::ir_helpers::IRHelper;

        // Validate LLM function tests
        for function in &self.functions {
            for test in &function.elem.tests {
                if let Some(span) = test.attributes.span.as_ref() {
                    self.validate_single_test_args(&function.elem, &test.elem, span, diagnostics);
                }
            }
        }

        // Validate expression function tests
        for expr_function in &self.expr_fns {
            for test in &expr_function.elem.tests {
                let pseudo_function = Function {
                    name: expr_function.elem.name.clone(),
                    inputs: expr_function.elem.inputs.clone(),
                    output: expr_function.elem.output.clone(),
                    tests: vec![],                 // Not used in validation
                    configs: vec![],               // Not used in validation
                    default_config: String::new(), // Not used in validation
                };
                if let Some(span) = test.attributes.span.as_ref() {
                    self.validate_single_test_args(&pseudo_function, &test.elem, span, diagnostics);
                }
            }
        }
    }

    fn validate_single_test_args(
        &self,
        function: &Function,
        test: &TestCase,
        test_span: &Span,
        diagnostics: &mut Diagnostics,
    ) {
        use std::collections::HashSet;

        use baml_types::BamlMap;
        use internal_baml_diagnostics::DatamodelError;

        use crate::ir::ir_helpers::IRHelper;

        let function_inputs: HashSet<&String> =
            function.inputs.iter().map(|(name, _)| name).collect();
        let test_args: HashSet<&String> = test.args.keys().collect();

        // Find missing required arguments (filter out optional/nullable types)
        let missing_args: Vec<&String> = function_inputs
            .difference(&test_args)
            .filter(|name| {
                function
                    .inputs
                    .iter()
                    .find(|(input_name, _)| *input_name == name.to_string())
                    .map(|(_, type_ir)| !type_ir.is_optional())
                    .unwrap_or(false)
            })
            .copied()
            .collect();

        // Handle missing arguments
        if !missing_args.is_empty() {
            if test.args.is_empty() && !function.inputs.is_empty() {
                // Test has empty args block but function has required args - provide full example
                let params_map: BamlMap<String, TypeIR> = function
                    .inputs
                    .iter()
                    .map(|(name, type_ir)| (name.clone(), type_ir.clone()))
                    .collect();
                let example_args = self.get_dummy_args(1, true, &params_map);

                diagnostics.push_warning(DatamodelWarning::new(
                    format!("Test '{}' is missing required arguments for function '{}'. Add an args block like:\n\nargs {{\n{}\n}}",
                             test.name, function.name, example_args),
                    test_span.clone(),
                ));
            } else if !missing_args.is_empty() {
                // Test has some args but is missing others - show missing ones with dummy values
                let missing_params_map: BamlMap<String, TypeIR> = missing_args
                    .iter()
                    .filter_map(|name| {
                        function
                            .inputs
                            .iter()
                            .find(|(input_name, _)| input_name == *name)
                            .map(|(name, type_ir)| (name.clone(), type_ir.clone()))
                    })
                    .collect();
                let missing_examples = self.get_dummy_args(0, false, &missing_params_map);

                diagnostics.push_warning(DatamodelWarning::new(
                    format!(
                        "Test '{}' is missing required arguments for function '{}': {}",
                        test.name,
                        function.name,
                        missing_examples.replace('\n', ", ")
                    ),
                    test_span.clone(),
                ));
            }
        }
    }

    /// Some block_types like enums and classes may have attributes on them.
    /// Every reference to them MUST also maintain that attribute.
    fn distribute_attributes(&mut self) {
        // first store all types that have block level attributes
        self.set_pass2_repr();

        // Now for every type every used in the IR, inject block level attributes
        // from the types that have them.

        // Special handling for classes with @@stream.done:
        // Fields within such classes should get both @done and @not_null
        for c in self.classes.iter_mut() {
            let class_streaming_behavior = c.attributes.streaming_behavior();

            // Only process if the class has @stream.done
            if class_streaming_behavior.done {
                for f in c.elem.static_fields.iter_mut() {
                    let field_type = &mut f.elem.r#type.elem;
                    field_type.meta_mut().streaming_behavior.done = true;
                    field_type.meta_mut().streaming_behavior.needed = true;
                }
            }
        }

        // finding types used in classes
        let class_fields = self.classes.iter_mut().flat_map(|c| {
            c.elem
                .static_fields
                .iter_mut()
                .map(|f| &mut f.elem.r#type.elem)
        });

        // finding types used in type aliases
        let type_alias_fields = self
            .structural_recursive_alias_cycles
            .iter_mut()
            .flat_map(|c| c.iter_mut().map(|(_, t)| t));

        // finding types used in functions
        let function_fields = self.functions.iter_mut().flat_map(|f| {
            f.elem
                .inputs
                .iter_mut()
                .map(|(_, t)| t)
                .chain(std::iter::once(&mut f.elem.output))
        });

        let all_types = class_fields.chain(type_alias_fields).chain(function_fields);

        // distribute attributes to all types
        all_types.for_each(|t| {
            self.pass2_repr.update_type(t);
        });
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
        Vec<IndexMap<String, TypeIR>>,
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

    /// Identifies type aliases that should be converted to interfaces in TypeScript
    /// to break circular reference cycles.
    ///
    /// TypeScript allows circular references in interfaces through object properties
    /// but not in direct type alias unions. This method identifies aliases that:
    /// 1. Are part of a structural recursive cycle
    /// 2. Can be safely converted to interfaces (object-like types)
    /// 3. Are referenced in ways that would cause TS circular reference errors
    pub fn get_typescript_interface_extractable_aliases(&self) -> Vec<String> {
        let mut extractable = Vec::new();

        // Check all structural recursive alias cycles
        for cycle in &self.structural_recursive_alias_cycles {
            for (alias_name, field_type) in cycle {
                if self.should_extract_as_typescript_interface(alias_name, field_type, cycle) {
                    extractable.push(alias_name.clone());
                }
            }
        }

        // Also check regular type aliases that directly reference themselves
        for alias in self.walk_type_aliases() {
            let alias_name = &alias.item.elem.name;
            let field_type = &alias.item.elem.r#type.elem;

            // Check if this alias directly references itself (causes immediate circular reference)
            if self.type_directly_references_self(field_type, alias_name) {
                // Check if it can be safely converted to an interface
                if self.can_convert_self_referencing_alias_to_interface(field_type) {
                    extractable.push(alias_name.clone());
                }
            }
        }

        extractable.sort();
        extractable.dedup();
        extractable
    }

    /// Checks if a self-referencing type alias can be safely converted to an interface
    fn can_convert_self_referencing_alias_to_interface(&self, field_type: &TypeIR) -> bool {
        use baml_types::ir_type::TypeGeneric;

        match field_type {
            // Object-like types (maps) can be converted
            TypeGeneric::Map(_, _, _) => true,

            // Unions containing maps can be converted
            TypeGeneric::Union(union_type, _) => union_type
                .iter_skip_null()
                .into_iter()
                .any(|t| matches!(t, TypeGeneric::Map(_, _, _))),

            // Other types cannot be safely converted
            _ => false,
        }
    }

    /// Determines if a specific type alias should be extracted as a TypeScript interface
    fn should_extract_as_typescript_interface(
        &self,
        alias_name: &str,
        field_type: &TypeIR,
        cycle: &IndexMap<String, TypeIR>,
    ) -> bool {
        use baml_types::ir_type::TypeGeneric;

        match field_type {
            // Object-like types (maps) can be converted to interfaces
            TypeGeneric::Map(_, _, _) => {
                // Check if this map references other types in the cycle
                self.type_references_cycle_members(field_type, cycle)
            }

            // Unions that contain object-like types can potentially be extracted
            TypeGeneric::Union(union_type, _) => {
                // Check if the union contains maps/objects and references cycle members
                let has_object_like = union_type
                    .iter_skip_null()
                    .into_iter()
                    .any(|t| matches!(t, TypeGeneric::Map(_, _, _)));
                let references_cycle = self.type_references_cycle_members(field_type, cycle);

                // For unions, we should extract if:
                // 1. It has object-like types (maps) AND references cycle members, OR
                // 2. It directly references itself (which causes circular reference in TS)
                has_object_like && references_cycle
                    || self.type_directly_references_self(field_type, alias_name)
            }

            // Don't extract primitive arrays, primitives, etc.
            TypeGeneric::List(_, _) | TypeGeneric::Primitive(_, _) | TypeGeneric::Literal(_, _) => {
                false
            }

            // Classes and enums are already interfaces/enums in TS
            TypeGeneric::Class { .. } | TypeGeneric::Enum { .. } => false,

            // Recursive references should be extracted if they're in problematic positions
            TypeGeneric::RecursiveTypeAlias { name, .. } => cycle.contains_key(name),

            _ => false,
        }
    }

    /// Checks if a type directly references itself (causing immediate circular reference)
    fn type_directly_references_self(&self, field_type: &TypeIR, alias_name: &str) -> bool {
        use baml_types::ir_type::TypeGeneric;

        fn check_type_inner(field_type: &TypeIR, alias_name: &str) -> bool {
            match field_type {
                TypeGeneric::RecursiveTypeAlias { name, .. } => name == alias_name,

                TypeGeneric::Union(union_type, _) => union_type
                    .iter_skip_null()
                    .into_iter()
                    .any(|t| check_type_inner(t, alias_name)),

                TypeGeneric::Map(key, value, _) => {
                    check_type_inner(key, alias_name) || check_type_inner(value, alias_name)
                }

                TypeGeneric::List(inner, _) => check_type_inner(inner, alias_name),

                _ => false,
            }
        }

        check_type_inner(field_type, alias_name)
    }

    /// Checks if a type references any members of the given cycle
    fn type_references_cycle_members(
        &self,
        field_type: &TypeIR,
        cycle: &IndexMap<String, TypeIR>,
    ) -> bool {
        use baml_types::ir_type::TypeGeneric;

        fn check_cycle_inner(field_type: &TypeIR, cycle: &IndexMap<String, TypeIR>) -> bool {
            match field_type {
                TypeGeneric::RecursiveTypeAlias { name, .. } => cycle.contains_key(name),

                TypeGeneric::List(inner, _) => check_cycle_inner(inner, cycle),

                TypeGeneric::Map(key, value, _) => {
                    check_cycle_inner(key, cycle) || check_cycle_inner(value, cycle)
                }

                TypeGeneric::Union(union_type, _) => union_type
                    .iter_skip_null()
                    .into_iter()
                    .any(|t| check_cycle_inner(t, cycle)),

                TypeGeneric::Tuple(types, _) => types.iter().any(|t| check_cycle_inner(t, cycle)),

                _ => false,
            }
        }

        check_cycle_inner(field_type, cycle)
    }

    /// Gets a mapping of alias names to whether they should be interfaces in TypeScript
    pub fn get_typescript_alias_conversion_map(&self) -> std::collections::HashMap<String, bool> {
        let extractable = self.get_typescript_interface_extractable_aliases();
        let mut conversion_map = std::collections::HashMap::new();

        // All type aliases default to type aliases
        for alias in self.walk_type_aliases() {
            conversion_map.insert(alias.item.elem.name.clone(), false);
        }

        // All recursive alias cycle types default to type aliases
        for cycle in &self.structural_recursive_alias_cycles {
            for alias_name in cycle.keys() {
                conversion_map.insert(alias_name.clone(), false);
            }
        }

        // Mark extractable ones as interfaces
        for alias_name in extractable {
            conversion_map.insert(alias_name, true);
        }

        conversion_map
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

fn is_some_true(maybe_value: Option<&UnresolvedValue<()>>) -> bool {
    matches!(maybe_value, Some(Resolvable::Bool(true, _)))
}

impl NodeAttributes {
    pub fn get(&self, key: &str) -> Option<&UnresolvedValue<()>> {
        self.meta.get(key)
    }

    pub fn dynamic(&self) -> bool {
        is_some_true(self.get("dynamic_type"))
    }

    pub fn alias(&self) -> Option<&baml_types::StringOr> {
        self.get("alias").and_then(|v| v.as_str())
    }

    pub fn description(&self) -> Option<&baml_types::StringOr> {
        self.get("description").and_then(|v| v.as_str())
    }

    pub fn skip(&self) -> bool {
        is_some_true(self.get("skip"))
    }

    pub fn streaming_behavior(&self) -> type_meta::base::StreamingBehavior {
        type_meta::base::StreamingBehavior {
            needed: is_some_true(self.get("stream.not_null")),
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
    .flatten()
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

fn type_with_arity(t: TypeIR, arity: &FieldArity) -> TypeIR {
    match arity {
        FieldArity::Required => t,
        FieldArity::Optional => t.as_optional(),
    }
}

impl WithRepr<TypeIR> for ast::FieldType {
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
            .any(|Attribute { name, .. }| name.name() == "stream.done")
        {
            let val: UnresolvedValue<()> = Resolvable::Bool(true, ());
            meta.insert("stream.done".to_string(), val);
        }
        if self
            .attributes()
            .iter()
            .any(|Attribute { name, .. }| name.name() == "stream.with_state")
        {
            let val: UnresolvedValue<()> = Resolvable::Bool(true, ());
            meta.insert("stream.with_state".to_string(), val);
        }
        if self
            .attributes()
            .iter()
            .any(|Attribute { name, .. }| name.name() == "stream.not_null")
        {
            let val: UnresolvedValue<()> = Resolvable::Bool(true, ());
            meta.insert("stream.not_null".to_string(), val);
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

    fn repr(&self, db: &ParserDatabase) -> Result<TypeIR> {
        let attributes = WithRepr::attributes(self, db);
        let has_constraints = !attributes.constraints.is_empty();
        let streaming_behavior = attributes.streaming_behavior();
        let has_special_streaming_behavior = streaming_behavior != Default::default();
        let mut base = match self {
            ast::FieldType::Primitive(arity, typeval, ..) => {
                let repr = TypeIR::Primitive(*typeval, Default::default());
                if arity.is_optional() {
                    repr.as_optional()
                } else {
                    repr
                }
            }
            ast::FieldType::Literal(arity, literal_value, ..) => {
                let repr = TypeIR::Literal(literal_value.clone(), Default::default());
                if arity.is_optional() {
                    repr.as_optional()
                } else {
                    repr
                }
            }
            ast::FieldType::Symbol(arity, idn, ..) => type_with_arity(
                match db.find_type(idn) {
                    Some(TypeWalker::Class(class_walker)) => {
                        let mut base_class = TypeIR::class(class_walker.name());
                        match class_walker.get_constraints(SubType::Class) {
                            Some(constraints) if !constraints.is_empty() => {
                                base_class.set_meta(type_meta::base::TypeMeta {
                                    constraints,
                                    streaming_behavior: streaming_behavior.clone(),
                                });
                                base_class
                            }
                            _ => base_class,
                        }
                    }
                    Some(TypeWalker::Enum(enum_walker)) => {
                        let mut base_type = TypeIR::r#enum(enum_walker.name());
                        match enum_walker.get_constraints(SubType::Enum) {
                            Some(constraints) if !constraints.is_empty() => {
                                base_type.set_meta(type_meta::base::TypeMeta {
                                    constraints,
                                    streaming_behavior: streaming_behavior.clone(),
                                });
                                base_type
                            }
                            _ => base_type,
                        }
                    }
                    Some(TypeWalker::TypeAlias(alias_walker)) => {
                        if db.is_recursive_type_alias(&alias_walker.id) {
                            let resolved = alias_walker.resolved();
                            // TODO: use resolved in some way
                            TypeIR::RecursiveTypeAlias {
                                name: alias_walker.name().to_string(),
                                mode: StreamingMode::Streaming,
                                meta: Default::default(),
                            }
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
                let mut repr = TypeIR::List(Box::new(ft.repr(db)?), Default::default());

                for _ in 1u32..*dims {
                    repr = TypeIR::list(repr);
                }

                if arity.is_optional() {
                    repr = TypeIR::optional(repr);
                }

                repr
            }
            ast::FieldType::Map(arity, kv, ..) => {
                // NB: we can't just unpack (*kv) into k, v because that would require a move/copy
                let mut repr = TypeIR::Map(
                    Box::new((kv).0.repr(db)?),
                    Box::new((kv).1.repr(db)?),
                    Default::default(),
                );

                if arity.is_optional() {
                    repr = TypeIR::optional(repr);
                }

                repr
            }
            ast::FieldType::Union(arity, t, ..) => {
                // NB: preempt union flattening by mixing arity into union types
                let mut types = t.iter().map(|ft| ft.repr(db)).collect::<Result<Vec<_>>>()?;

                if arity.is_optional() {
                    types.push(TypeIR::Primitive(
                        baml_types::TypeValue::Null,
                        Default::default(),
                    ));
                }

                TypeIR::union(types)
            }
            ast::FieldType::Tuple(arity, t, ..) => type_with_arity(
                TypeIR::Tuple(
                    t.iter().map(|ft| ft.repr(db)).collect::<Result<Vec<_>>>()?,
                    Default::default(),
                ),
                arity,
            ),
        };

        let use_metadata = has_constraints || has_special_streaming_behavior;
        let with_constraints = if use_metadata {
            base.set_meta(type_meta::base::TypeMeta {
                constraints: attributes.constraints,
                streaming_behavior: streaming_behavior.clone(),
            });
            base
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
    pub r#type: Node<TypeIR>,
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
    /// Note that this is a future feature, not something we currently use.
    pub inputs: Vec<(String, TypeIR)>,

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
    pub fn inputs(&self) -> &Vec<(String, TypeIR)> {
        &self.inputs
    }
}

#[derive(Clone, Debug)]
pub struct TypeAlias {
    pub name: String,
    pub r#type: Node<TypeIR>,
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
    UnnamedArg(TypeIR),
    NamedArgList(Vec<(String, TypeIR)>),
}

type FunctionId = String;

impl Function {
    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn output(&self) -> &TypeIR {
        &self.output
    }

    pub fn inputs(&self) -> &Vec<(String, TypeIR)> {
        &self.inputs
    }

    pub fn tests(&self) -> &Vec<Node<TestCase>> {
        &self.tests
    }

    pub fn configs(&self) -> Option<&Vec<FunctionConfig>> {
        Some(&self.configs)
    }

    pub fn default_config(&self) -> Option<&FunctionConfig> {
        self.configs.iter().find(|c| c.name == self.default_config)
    }
}

#[derive(Debug)]
pub struct Function {
    pub name: FunctionId,
    pub inputs: Vec<(String, TypeIR)>,
    pub output: TypeIR,
    pub tests: Vec<Node<TestCase>>,
    pub configs: Vec<FunctionConfig>,
    pub default_config: String,
}

#[derive(Clone, Debug)]
pub struct FunctionConfig {
    pub name: String,
    pub prompt_template: String,
    pub prompt_span: ast::Span,
    pub client: ClientSpec,
}

#[derive(Clone, Debug)]
pub struct ExprFunction {
    pub name: FunctionId,
    pub inputs: Vec<(String, TypeIR)>,
    pub output: TypeIR,
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
            configs: vec![FunctionConfig {
                name: "default_config".to_string(),
                prompt_template: "".to_string(),
                prompt_span: Span::fake(),
                client: ClientSpec::Named("nonsense".to_string()),
            }],
            default_config: "default_config".to_string(),
        }
    }

    pub fn inputs(&self) -> &Vec<(String, TypeIR)> {
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

                Expr::Lambda(*arity, Arc::new(new_body), meta.clone())
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
    r#type: TypeIR,
    expr: Expr<ExprMetadata>,
) -> Expr<ExprMetadata> {
    match &expr {
        Expr::FreeVar(var_name, meta) => expr,
        Expr::Builtin(builtin, meta) => Expr::Builtin(builtin.clone(), meta.clone()),
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
        Expr::App {
            func,
            args,
            meta,
            type_args,
        } => {
            let new_f = annotate_variable(
                target.clone(),
                r#type.clone(),
                Arc::unwrap_or_clone(func.clone()),
            );
            let new_args =
                annotate_variable(target.clone(), r#type, Arc::unwrap_or_clone(args.clone()));
            Expr::App {
                func: Arc::new(new_f),
                args: Arc::new(new_args),
                meta: meta.clone(),
                type_args: type_args.clone(),
            }
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
            let new_spread = spread.as_ref().map(|expr| {
                Box::new(annotate_variable(
                    target,
                    r#type.clone(),
                    expr.as_ref().clone(),
                ))
            });
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
        Expr::If(cond, then, else_, meta) => {
            let new_cond = annotate_variable(
                target.clone(),
                r#type.clone(),
                Arc::unwrap_or_clone(cond.clone()),
            );
            let new_then = annotate_variable(
                target.clone(),
                r#type.clone(),
                Arc::unwrap_or_clone(then.clone()),
            );
            let new_else = else_.as_ref().map(|e| {
                annotate_variable(
                    target.clone(),
                    r#type.clone(),
                    Arc::unwrap_or_clone(e.clone()),
                )
            });
            Expr::If(
                Arc::new(new_cond),
                Arc::new(new_then),
                new_else.map(Arc::new),
                meta.clone(),
            )
        }
        Expr::ForLoop {
            item,
            iterable,
            body,
            meta,
        } => {
            let new_iterable = annotate_variable(
                target.clone(),
                r#type.clone(),
                Arc::unwrap_or_clone(iterable.clone()),
            );
            let new_body = annotate_variable(
                target.clone(),
                r#type.clone(),
                Arc::unwrap_or_clone(body.clone()),
            );
            Expr::ForLoop {
                item: item.clone(),
                iterable: Arc::new(new_iterable),
                body: Arc::new(new_body),
                meta: meta.clone(),
            }
        }
        Expr::ArrayAccess { base, index, meta } => {
            let new_base = annotate_variable(
                target.clone(),
                r#type.clone(),
                Arc::unwrap_or_clone(base.clone()),
            );
            let new_index = annotate_variable(
                target.clone(),
                r#type.clone(),
                Arc::unwrap_or_clone(index.clone()),
            );
            Expr::ArrayAccess {
                base: Arc::new(new_base),
                index: Arc::new(new_index),
                meta: meta.clone(),
            }
        }
        Expr::FieldAccess { base, field, meta } => {
            let new_base = annotate_variable(
                target.clone(),
                r#type.clone(),
                Arc::unwrap_or_clone(base.clone()),
            );
            Expr::FieldAccess {
                base: Arc::new(new_base),
                field: field.clone(),
                meta: meta.clone(),
            }
        }
        Expr::BinaryOperation {
            left,
            right,
            operator,
            meta,
        } => {
            let new_left = annotate_variable(
                target.clone(),
                r#type.clone(),
                Arc::unwrap_or_clone(left.clone()),
            );
            let new_right = annotate_variable(
                target.clone(),
                r#type.clone(),
                Arc::unwrap_or_clone(right.clone()),
            );
            Expr::BinaryOperation {
                left: Arc::new(new_left),
                operator: operator.clone(),
                right: Arc::new(new_right),
                meta: meta.clone(),
            }
        }
        Expr::UnaryOperation {
            expr,
            operator,
            meta,
        } => {
            let new_expr = annotate_variable(
                target.clone(),
                r#type.clone(),
                Arc::unwrap_or_clone(expr.clone()),
            );
            Expr::UnaryOperation {
                expr: Arc::new(new_expr),
                operator: operator.clone(),
                meta: meta.clone(),
            }
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

            #[allow(clippy::map_entry)] // can't use map.entry() without cloning spans here
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
    pub options: Vec<(String, UnresolvedValue<()>)>,
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
    pub recursive_aliases: Vec<IndexMap<String, TypeIR>>,
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
        Err(anyhow::anyhow!(
            "Source code was invalid: \n{:?}",
            diagnostics.errors()
        ))
    } else {
        Ok(ir)
    }
}

pub fn make_test_ir_from_dir(dir: &std::path::PathBuf) -> anyhow::Result<IntermediateRepr> {
    // load all *.baml files in the directory
    let files = std::fs::read_dir(dir)?
        .filter_map(|file| file.ok())
        .filter(|file| file.path().extension().is_some_and(|ext| ext == "baml"))
        .map(|file| file.path())
        .map(|path| Ok((path.clone(), std::fs::read_to_string(path)?).into()))
        .collect::<Result<Vec<_>>>()?;

    let (ir, diagnostics) = make_test_ir_and_diagnostics_from_dir(dir, files)?;
    if diagnostics.has_errors() {
        return Err(anyhow::anyhow!(
            "Source code was invalid: \n{:?}",
            diagnostics.errors()
        ));
    }
    Ok(ir)
}

/// Generate an IntermediateRepr from a single block of BAML source code.
/// This is useful for generating IR test fixtures. Also return the
/// `Diagnostics`.
pub fn make_test_ir_and_diagnostics(
    source_code: &str,
) -> anyhow::Result<(IntermediateRepr, Diagnostics)> {
    use std::path::PathBuf;

    use internal_baml_diagnostics::SourceFile;

    use crate::{validate, ValidatedSchema};

    let path: PathBuf = "fake_file.baml".into();
    let source_file: SourceFile = (path.clone(), source_code).into();
    let validated_schema: ValidatedSchema =
        validate(&path, vec![source_file], crate::FeatureFlags::new());
    let diagnostics = validated_schema.diagnostics;
    let ir = IntermediateRepr::from_parser_database(
        &validated_schema.db,
        validated_schema.configuration,
    )?;
    Ok((ir, diagnostics))
}

/// Generate an IntermediateRepr from a single block of BAML source code.
/// This is useful for generating IR test fixtures. Also return the
/// `Diagnostics`.
fn make_test_ir_and_diagnostics_from_dir(
    root_dir: &std::path::Path,
    source_code: Vec<internal_baml_diagnostics::SourceFile>,
) -> anyhow::Result<(IntermediateRepr, Diagnostics)> {
    use std::path::PathBuf;

    use internal_baml_diagnostics::SourceFile;

    use crate::{validate, ValidatedSchema};

    let validated_schema: ValidatedSchema =
        validate(root_dir, source_code, crate::FeatureFlags::new());
    let diagnostics = validated_schema.diagnostics;
    let ir = IntermediateRepr::from_parser_database(
        &validated_schema.db,
        validated_schema.configuration,
    )?;
    Ok((ir, diagnostics))
}

// Specialize generics.
fn specialize_generics(expr: &Expr<ExprMetadata>, ctx: &mut HashMap<Name, Expr<ExprMetadata>>) {
    match expr {
        Expr::FreeVar(name, _) => {}
        Expr::BoundVar(name, _) => {}
        Expr::Builtin(_, _) => {}
        Expr::Atom(_) => {}
        Expr::Let(name, expr, body, _) => {
            specialize_generics(expr, ctx);
            specialize_generics(body, ctx);
        }
        Expr::Lambda(_, body, _) => {
            specialize_generics(body, ctx);
        }
        Expr::ArgsTuple(exprs, _) => {
            for expr in exprs {
                specialize_generics(expr, ctx);
            }
        }
        Expr::LLMFunction(_, _, _) => {}
        Expr::List(exprs, _) => {
            for expr in exprs {
                specialize_generics(expr, ctx);
            }
        }
        Expr::Map(exprs, _) => {
            for (_, expr) in exprs {
                specialize_generics(expr, ctx);
            }
        }
        Expr::ClassConstructor {
            fields,
            spread,
            meta,
            ..
        } => {
            for expr in fields.values() {
                specialize_generics(expr, ctx);
            }
            if let Some(expr) = spread {
                specialize_generics(expr, ctx);
            }
        }
        Expr::App {
            func,
            type_args,
            args,
            meta,
        } => {
            // If there's a type arg then we know it's a builtin function
            // because as of right now users can't define their own generic
            // functions. We also know that the name is already mangled because
            // we do that when we build the IR from the AST. Take a look at
            // WithRepr<Expr> for ast::Expression::App for more details.
            if let Some(type_arg) = type_args.first() {
                if let Expr::FreeVar(name, _) = func.as_ref() {
                    ctx.insert(
                        name.clone(), // Already mangled.
                        builtin_generic_fn(Builtin::FetchValue, type_arg.clone()),
                    );
                }
            }
            specialize_generics(args, ctx);
        }
        Expr::If(cond, then, r#else, meta) => {
            specialize_generics(cond, ctx);
            specialize_generics(then, ctx);
            if let Some(r#else) = r#else {
                specialize_generics(r#else, ctx);
            }
        }
        Expr::ForLoop { iterable, body, .. } => {
            specialize_generics(iterable, ctx);
            specialize_generics(body, ctx);
        }
        Expr::ArrayAccess { base, index, .. } => {
            specialize_generics(base, ctx);
            specialize_generics(index, ctx);
        }
        Expr::FieldAccess { base, .. } => {
            specialize_generics(base, ctx);
        }
        Expr::BinaryOperation {
            left,
            operator,
            right,
            ..
        } => {
            specialize_generics(left, ctx);
            specialize_generics(right, ctx);
        }
        Expr::UnaryOperation { expr, .. } => {
            specialize_generics(expr, ctx);
        }
    }
}

/// Create a context from the expr_functions, top_level_assignments, and
/// functions in the IR.
/// This context is used in evaluating expressions.
pub fn initial_context(ir: &IntermediateRepr) -> HashMap<Name, Expr<ExprMetadata>> {
    let mut ctx = HashMap::new();

    for expr_fn in ir.expr_fns.iter() {
        ctx.insert(expr_fn.elem.name.clone(), expr_fn.elem.expr.clone());
        specialize_generics(&expr_fn.elem.expr, &mut ctx);
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
        let params_type: Vec<TypeIR> = llm_function
            .elem
            .inputs
            .iter()
            .map(|arg| arg.1.clone())
            .collect::<Vec<_>>();
        let body_type = llm_function.elem.output.clone();
        let lambda_type = TypeIR::Arrow(
            Box::new(ArrowGeneric {
                param_types: params_type,
                return_type: body_type,
            }),
            Default::default(),
        );
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

pub fn initial_typing_context(ir: &IntermediateRepr) -> HashMap<Name, TypeIR> {
    let ctx = initial_context(ir);
    ctx.into_iter()
        .filter_map(|(name, expr)| expr.meta().1.as_ref().map(|t| (name, t.clone())))
        .collect()
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
        assert!(!foo.streaming_behavior().done);
        match foo.walk_fields().collect::<Vec<_>>().as_slice() {
            [field1, field2, field3] => {
                let type1 = &field1.item.elem.r#type;
                assert!(type1.attributes.streaming_behavior().needed);
                let type2 = &field2.item.elem.r#type;
                assert!(!field2.streaming_behavior().state);
                assert!(type2.attributes.get("stream.with_state").is_some());
                let type3 = &field3.item.elem.r#type;
                // the field doesnt have this attribute / behavior -- the type does. But we should document why somewhere better.
                assert!(!field3.streaming_behavior().done);
                assert!(type3.attributes.get("stream.done").is_some());
            }
            _ => panic!("Expected exactly 3 fields"),
        }
        let bar = ir.find_class("Bar").unwrap();
        assert!(bar.streaming_behavior().done);
        match bar.walk_fields().collect::<Vec<_>>().as_slice() {
            [field1, field2] => {
                assert!(!field1.streaming_behavior().done);
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

        assert_eq!(*alias.r#type(), TypeIR::int());
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

        let type_meta::base::TypeMeta { constraints, .. } = alias.r#type().meta();

        assert_eq!(constraints.len(), 3);

        assert_eq!(constraints[0].level, ConstraintLevel::Assert);
        assert_eq!(constraints[0].label, None);

        assert_eq!(constraints[1].level, ConstraintLevel::Check);
        assert_eq!(constraints[1].label, Some("lt_twenty".to_string()));

        assert_eq!(constraints[2].level, ConstraintLevel::Check);
        assert_eq!(constraints[2].label, Some("gt_ten".to_string()));
    }

    #[test]
    fn test_recursive_type_resolution_consistency() {
        for _ in 0..1000 {
            let ir = make_test_ir(
                r##"
                type MyUnion = Recursive1 | Nonrecursive1 | Nonrecursive2
                type Recursive1 = int | Recursive1[]
                type Nonrecursive1 = int | null
                type Nonrecursive2 = (null | string) | null | (null | null)
                type MyUnion2 = Recursive1 | Nonrecursive1 | Nonrecursive2
                class UseMyUnion {
                    u MyUnion
                    u2 MyUnion2
                }
            "##,
            )
            .unwrap();

            let class = ir.find_class("UseMyUnion").unwrap();
            let field1 = class.find_field("u").unwrap();
            let field1_type = &field1.elem().r#type.elem;

            let field2 = class.find_field("u2").unwrap();
            let field2_type = &field2.elem().r#type.elem;

            // Both fields should have consistent type resolution for Recursive1
            assert_eq!(
                field1_type.to_string(),
                "(Streaming.Recursive1 | int @stream.done | string | null)", // Union3IntOrRecursive1OrString
                "field1 type resolution is inconsistent"
            );
            assert_eq!(
                field2_type.to_string(),
                "(Streaming.Recursive1 | int @stream.done | string | null)", // Union3IntOrRecursive1OrString
                "field2 type resolution is inconsistent"
            );
        }
    }

    #[test]
    fn test_recursive_type_resolution_consistency_with_different_top_level_names() {
        for _ in 0..1000 {
            let ir = make_test_ir(
                r##"
                type ZMyUnion = Recursive1 | Nonrecursive1 | Nonrecursive2
                type Recursive1 = int | Recursive1[]
                type Nonrecursive1 = int | null
                type Nonrecursive2 = (null | string) | null | (null | null)
                type MyUnion2 = Recursive1 | Nonrecursive1 | Nonrecursive2
                class UseMyUnion {
                    u ZMyUnion
                    u2 MyUnion2
                }
            "##,
            )
            .unwrap();

            let class = ir.find_class("UseMyUnion").unwrap();
            let field1 = class.find_field("u").unwrap();
            let field1_type = &field1.elem().r#type.elem;

            let field2 = class.find_field("u2").unwrap();
            let field2_type = &field2.elem().r#type.elem;

            // Both fields should have consistent type resolution for Recursive1
            assert_eq!(
                field1_type.to_string(),
                "(int @stream.done | Streaming.Recursive1 @stream.not_null[] @stream.not_null | string | null)", // Union3IntOrRecursive1OrString
                "field1 type resolution is inconsistent"
            );
            assert_eq!(
                field2_type.to_string(),
                "(Streaming.Recursive1 | int @stream.done | string | null)", // Union3IntOrRecursive1OrString
                "field2 type resolution is inconsistent"
            );
        }
    }

    #[test]
    fn test_expr_fn_tests() {
        let ir = make_test_ir(
            r##"
            function Foo(x: int) -> int {
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

    #[test]
    fn test_typescript_interface_extraction() {
        let ir = make_test_ir(
            r##"
            type JsonValue = int | string | bool | JsonObject | JsonArray
            type JsonObject = map<string, JsonValue>
            type JsonArray = JsonValue[]
            type SimpleAlias = int
        "##,
        )
        .unwrap();

        let extractable_aliases = ir.get_typescript_interface_extractable_aliases();
        let conversion_map = ir.get_typescript_alias_conversion_map();

        // Basic test: conversion map should exist for all type aliases
        assert!(conversion_map.contains_key("SimpleAlias"));

        // If there are cycles detected, then test those
        if !ir.structural_recursive_alias_cycles.is_empty() {
            // JsonObject should be extractable as it's a map type in a cycle
            assert!(
                extractable_aliases.contains(&"JsonObject".to_string())
                    || conversion_map.contains_key("JsonObject")
            );
        }
    }

    #[test]
    fn test_typescript_recursive_alias_detection() {
        let ir = make_test_ir(
            r##"
            type RecursiveList = RecursiveList[]
            type RecursiveMap = map<string, RecursiveMap>
            type RecursiveUnion = string | map<string, RecursiveUnion>
            type NormalAlias = string
        "##,
        )
        .unwrap();

        let extractable_aliases = ir.get_typescript_interface_extractable_aliases();
        let conversion_map = ir.get_typescript_alias_conversion_map();

        // RecursiveMap should be extractable as it's an object-like type
        assert!(extractable_aliases.contains(&"RecursiveMap".to_string()));
        assert_eq!(conversion_map.get("RecursiveMap"), Some(&true));

        // RecursiveUnion should be extractable as it contains a map and references itself
        assert!(extractable_aliases.contains(&"RecursiveUnion".to_string()));
        assert_eq!(conversion_map.get("RecursiveUnion"), Some(&true));

        // RecursiveList should remain as type alias as it's array-based
        assert_eq!(conversion_map.get("RecursiveList"), Some(&false));

        // NormalAlias should remain as type alias
        assert_eq!(conversion_map.get("NormalAlias"), Some(&false));
    }
}
