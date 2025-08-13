/// Type-checked HIR.
///
use crate::hir::{BinaryOperator, Class, Enum, LlmFunction, Type, UnaryOperator};

pub mod typecheck;

use std::{
    collections::{HashSet, VecDeque},
    sync::Arc,
};

use baml_types::{BamlMap, BamlValueWithMeta};
use internal_baml_diagnostics::Span;
use itertools::join;

/// A full BAML program.
/// This differs from HIR in a few ways:
///   - Expressions are (optionally) typed.
///   - Variables are bound or free, using the locally nameless representation.
#[derive(Clone, Debug)]
pub struct THir<T> {
    pub expr_functions: Vec<ExprFunction<T>>,
    pub llm_functions: Vec<LlmFunction>,
    pub global_assignments: BamlMap<String, Expr<ExprMetadata>>,
    pub classes: BamlMap<String, Class>,
    pub enums: BamlMap<String, Enum>,
}

#[derive(Clone, Debug)]
pub struct ExprFunction<T> {
    pub name: String,
    pub parameters: Vec<Parameter>,
    pub return_type: Type,
    pub body: Block<T>,
    pub span: Span,
}

#[derive(Clone, Debug)]
pub struct Parameter {
    pub name: String,
    pub r#type: Type,
    pub span: Span,
}

/// A BAML expression term.
/// T is the type of the metadata.
#[derive(Debug, Clone)]
pub enum Expr<T> {
    Atom(BamlValueWithMeta<T>),
    List(Vec<Expr<T>>, T),
    Map(BamlMap<String, Expr<T>>, T),
    Block(Box<Block<T>>, T),
    ClassConstructor {
        name: String,
        fields: BamlMap<String, Expr<T>>,
        spread: Option<Box<Expr<T>>>,
        meta: T,
    },
    // A free variable, not bound by a lambda.
    FreeVar(Name, T),
    // The DeBruijn index of a bound variable.
    BoundVar(VarIndex, T),
    Function(usize, Arc<Block<T>>, T), // number of parameters, body, metadata
    Call {
        func: Arc<Expr<T>>,
        type_args: Vec<Type>,
        args: Vec<Expr<T>>,
        meta: T,
    },
    If(Arc<Expr<T>>, Arc<Expr<T>>, Option<Arc<Expr<T>>>, T),
    Builtin(Builtin, T),
    ForLoop {
        item: Name, // An identifier. TODO: Generalize to left-hand-side. i.e. name or other pattern.
        iterable: Arc<Expr<T>>,
        body: Arc<Expr<T>>,
        meta: T,
    },
    /// Array or map access: `base[index]`
    ArrayAccess {
        base: Arc<Expr<T>>,
        index: Arc<Expr<T>>,
        meta: T,
    },
    /// Field access: `base.field`
    FieldAccess {
        base: Arc<Expr<T>>,
        field: String,
        meta: T,
    },
    BinaryOperation {
        left: Arc<Expr<T>>,
        operator: BinaryOperator,
        right: Arc<Expr<T>>,
        meta: T,
    },
    UnaryOperation {
        operator: UnaryOperator,
        expr: Arc<Expr<T>>,
        meta: T,
    },
}

/// A block of statements and a final return value.
#[derive(Clone, Debug)]
pub struct Block<T> {
    pub env: BamlMap<Variable, Expr<T>>,
    pub statements: Vec<Statement<T>>,
    pub return_value: Expr<T>,
    pub span: Span,
}

impl<T> Block<T> {
    pub fn dump_str(&self) -> String
    where
        T: Clone + std::fmt::Debug,
    {
        let statements = join(self.statements.iter().map(|stmt| stmt.dump_str()), "\n");
        format!("{{ {statements} }}")
    }

    pub fn free_vars(&self) -> HashSet<Name>
    where
        T: Clone,
    {
        let mut free_vars = self.return_value.free_vars();
        for stmt in self.statements.iter() {
            free_vars.extend(stmt.free_vars());
        }
        free_vars
    }

    pub fn open(&self, target: &VarIndex, new_name: &str) -> Block<T>
    where
        T: Clone + std::fmt::Debug,
    {
        Block {
            env: self.env.clone(),
            statements: self
                .statements
                .iter()
                .map(|stmt| stmt.open(target, new_name))
                .collect(),
            return_value: self.return_value.open(target, new_name),
            span: self.span.clone(),
        }
    }

    pub fn close(&self, new_index: &VarIndex, target: &str) -> Block<T>
    where
        T: Clone + std::fmt::Debug,
    {
        Block {
            env: self.env.clone(),
            statements: self
                .statements
                .iter()
                .map(|stmt| stmt.close(new_index, target))
                .collect(),
            return_value: self.return_value.close(new_index, target),
            span: self.span.clone(),
        }
    }

    pub fn temporary_same_state(&self, other: &Block<T>) -> bool
    where
        T: Clone + std::fmt::Debug,
    {
        self.return_value.temporary_same_state(&other.return_value)
            && self
                .statements
                .iter()
                .zip(other.statements.iter())
                .all(|(s1, s2)| s1.temporary_same_state(s2))
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Builtin {
    FetchValue,
}

pub type Name = String;

/// The locally-nameless index of a bound variable.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VarIndex {
    /// Locally nameless De Bruijn index of a variable.
    /// This is related to the number of lambda binders under
    /// which the variable is located.
    pub de_bruijn: u32,

    /// Our functions all take tuples of arguments, so the index of
    /// a bound variable must specify the tuple index in addition
    /// to the De Bruijn index.
    pub tuple: u32,
}

impl VarIndex {
    pub fn dump_str(&self) -> String {
        format!("({}.{})", self.de_bruijn, self.tuple)
    }
    pub fn deeper(&self) -> Self {
        VarIndex {
            de_bruijn: self.de_bruijn + 1,
            tuple: self.tuple,
        }
    }
}

/// The metadata used during parsing, typechecking and evaluation of BAML expressions.
pub type ExprMetadata = (Span, Option<Type>);

impl<T: Clone + std::fmt::Debug> Expr<T> {
    pub fn meta(&self) -> &T {
        match self {
            Expr::Atom(baml_value) => baml_value.meta(),
            Expr::Block(_, meta) => meta,
            Expr::List(_, meta) => meta,
            Expr::Map(_, meta) => meta,
            Expr::ClassConstructor { meta, .. } => meta,
            Expr::BoundVar(_, meta) => meta,
            Expr::FreeVar(_, meta) => meta,
            Expr::Function(_, _, meta) => meta,
            Expr::Call { meta, .. } => meta,
            Expr::Builtin(_, meta) => meta,
            Expr::If(_, _, _, meta) => meta,
            Expr::ForLoop { meta, .. } => meta,
            Expr::ArrayAccess { meta, .. } => meta,
            Expr::FieldAccess { meta, .. } => meta,
            Expr::BinaryOperation { meta, .. } => meta,
            Expr::UnaryOperation { meta, .. } => meta,
        }
    }

    pub fn meta_mut(&mut self) -> &mut T {
        match self {
            Expr::Atom(baml_value) => baml_value.meta_mut(),
            Expr::Block(_, meta) => meta,
            Expr::List(_, meta) => meta,
            Expr::Map(_, meta) => meta,
            Expr::ClassConstructor { meta, .. } => meta,
            Expr::BoundVar(_, meta) => meta,
            Expr::FreeVar(_, meta) => meta,
            Expr::Function(_, _, meta) => meta,
            Expr::Call { meta, .. } => meta,
            Expr::Builtin(_, meta) => meta,
            Expr::If(_, _, _, meta) => meta,
            Expr::ForLoop { meta, .. } => meta,
            Expr::ArrayAccess { meta, .. } => meta,
            Expr::FieldAccess { meta, .. } => meta,
            Expr::BinaryOperation { meta, .. } => meta,
            Expr::UnaryOperation { meta, .. } => meta,
        }
    }

    pub fn into_meta(self) -> T {
        match self {
            Expr::Atom(baml_value) => baml_value.meta().clone(),
            Expr::Block(_, meta) => meta,
            Expr::List(_, meta) => meta,
            Expr::Map(_, meta) => meta,
            Expr::ClassConstructor { meta, .. } => meta,
            Expr::BoundVar(_, meta) => meta,
            Expr::FreeVar(_, meta) => meta,
            Expr::Function(_, _, meta) => meta,
            Expr::Call { meta, .. } => meta,
            Expr::Builtin(_, meta) => meta,
            Expr::If(_, _, _, meta) => meta,
            Expr::ForLoop { meta, .. } => meta,
            Expr::ArrayAccess { meta, .. } => meta,
            Expr::FieldAccess { meta, .. } => meta,
            Expr::BinaryOperation { meta, .. } => meta,
            Expr::UnaryOperation { meta, .. } => meta,
        }
    }
}

impl<T: Clone + std::fmt::Debug> Expr<T> {
    /// A very rough pretty-printer for debugging expressions.
    pub fn dump_str(&self) -> String {
        match self {
            Expr::Atom(atom) => atom.clone().value().to_string(),
            Expr::Block(block, _) => block.dump_str(),
            Expr::BoundVar(ind, _) => ind.dump_str(),
            Expr::FreeVar(name, _) => name.clone(),
            Expr::Function(_, body, meta) => format!(
                "\\. -> {}",
                Expr::Block(Box::new(Arc::unwrap_or_clone(body.clone())), meta.clone()).dump_str()
            ),
            Expr::Call { func, args, .. } => {
                let args_str = itertools::join(args.iter().map(|arg| arg.dump_str()), ", ");
                let func_str = match func.as_ref() {
                    Expr::BoundVar(ind, _) => ind.dump_str(),
                    Expr::FreeVar(name, _) => name.clone(),
                    _ => format!("({})", func.dump_str()),
                };
                format!("{func_str}({args_str})")
            }
            Expr::Builtin(builtin, _) => format!("{builtin:?}"),
            Expr::List(items, _) => {
                let items = join(
                    items.iter().map(|item| item.dump_str()).collect::<Vec<_>>(),
                    ", ",
                );
                format!("[{items}]")
            }
            Expr::Map(entries, _) => {
                let entries = entries
                    .iter()
                    .map(|(key, value)| format!("{}: {}", key, value.dump_str()))
                    .collect::<Vec<_>>()
                    .join(", ");
                format!("{{{entries}}}")
            }
            Expr::ClassConstructor {
                name,
                fields,
                spread,
                ..
            } => {
                let fields = fields
                    .iter()
                    .map(|(key, value)| format!("{}: {}", key, value.dump_str()))
                    .collect::<Vec<_>>()
                    .join(", ");
                let spread = match spread {
                    Some(expr) => format!("..{}", expr.dump_str()),
                    None => String::new(),
                };
                format!("Class({name} {{ {fields}{spread} }}")
            }
            Expr::If(cond, then, else_, _) => {
                format!(
                    "If {} {{ {} }} {}",
                    cond.dump_str(),
                    then.dump_str(),
                    else_.as_ref().map(|e| e.dump_str()).unwrap_or_default()
                )
            }
            Expr::ForLoop {
                item,
                iterable,
                body,
                ..
            } => {
                format!(
                    "For {} in {} {{ {} }}",
                    item,
                    iterable.dump_str(),
                    body.dump_str()
                )
            }
            Expr::ArrayAccess { base, index, .. } => {
                format!("{}[{}]", base.dump_str(), index.dump_str())
            }
            Expr::FieldAccess { base, field, .. } => {
                format!("{}.{}", base.dump_str(), field)
            }
            Expr::BinaryOperation {
                left,
                operator,
                right,
                ..
            } => format!("({} {} {})", left.dump_str(), operator, right.dump_str()),
            Expr::UnaryOperation { operator, expr, .. } => {
                format!("({} {})", operator, expr.dump_str())
            }
        }
    }

    /// This quick hack of a function checks whether two expressions are
    /// equal in terms of reduction state. This test is used to detect
    /// if the evaluation stepper is stuck.
    pub fn temporary_same_state(&self, other: &Expr<T>) -> bool {
        match (self, other) {
            (Expr::Atom(a1), Expr::Atom(a2)) => a1.clone().value() == a2.clone().value(),
            (Expr::Atom(_), _) => false,

            (Expr::Block(b1, _), Expr::Block(b2, _)) => b1.temporary_same_state(b2),
            (Expr::Block(_, _), _) => false,

            (Expr::Builtin(b1, _), Expr::Builtin(b2, _)) => b1 == b2,
            (Expr::Builtin(_, _), _) => false,

            (Expr::BoundVar(n1, _), Expr::BoundVar(n2, _)) => n1 == n2,
            (Expr::BoundVar(_, _), _) => false,

            (Expr::FreeVar(n1, _), Expr::FreeVar(n2, _)) => n1 == n2,
            (Expr::FreeVar(_, _), _) => false,

            (Expr::Function(name1, body1, _), Expr::Function(name2, body2, _)) => {
                name1 == name2 && body1.temporary_same_state(body2)
            }
            (Expr::Function(_, _, _), _) => false,
            (
                Expr::Call {
                    func: f1, args: x1, ..
                },
                Expr::Call {
                    func: f2, args: x2, ..
                },
            ) => {
                f1.temporary_same_state(f2)
                    && x1
                        .iter()
                        .zip(x2.iter())
                        .all(|(a1, a2)| a1.temporary_same_state(a2))
            }
            (Expr::Call { .. }, _) => false,

            (
                Expr::ClassConstructor {
                    name: n1,
                    fields: e1,
                    spread: s1,
                    ..
                },
                Expr::ClassConstructor {
                    name: n2,
                    fields: e2,
                    spread: s2,
                    ..
                },
            ) => {
                n1 == n2
                    && e1.len() == e2.len()
                    && e1
                        .iter()
                        .zip(e2.iter())
                        .all(|((_k1, v1), (_k2, v2))| v1.temporary_same_state(v2))
                    && (match (s1, s2) {
                        (Some(s1), Some(s2)) => s1.temporary_same_state(s2),
                        (None, None) => true,
                        _ => false,
                    })
            }
            (Expr::ClassConstructor { .. }, _) => false,

            (Expr::Map(e1, _), Expr::Map(e2, _)) => {
                e1.len() == e2.len()
                    && e1
                        .iter()
                        .zip(e2.iter())
                        .all(|((_k1, v1), (_k2, v2))| v1.temporary_same_state(v2))
            }
            (Expr::Map(_, _), _) => false,

            (Expr::List(e1, _), Expr::List(e2, _)) => {
                e1.len() == e2.len()
                    && e1
                        .iter()
                        .zip(e2.iter())
                        .all(|(a1, a2)| a1.temporary_same_state(a2))
            }
            (Expr::List(_, _), _) => false,

            (Expr::If(cond1, then1, else1, _), Expr::If(cond2, then2, else2, _)) => {
                let else_same = match (&else1, &else2) {
                    (Some(e1), Some(e2)) => e1.temporary_same_state(e2),
                    (None, None) => true,
                    _ => false,
                };
                cond1.temporary_same_state(cond2) && then1.temporary_same_state(then2) && else_same
            }
            (Expr::If(_, _, _, _), _) => false,
            (
                Expr::ForLoop {
                    item: i1,
                    iterable: iter1,
                    body: body1,
                    ..
                },
                Expr::ForLoop {
                    item: i2,
                    iterable: iter2,
                    body: body2,
                    ..
                },
            ) => i1 == i2 && iter1.temporary_same_state(iter2) && body1.temporary_same_state(body2),
            (Expr::ForLoop { .. }, _) => false,
            (
                Expr::ArrayAccess {
                    base: base1,
                    index: index1,
                    ..
                },
                Expr::ArrayAccess {
                    base: base2,
                    index: index2,
                    ..
                },
            ) => base1.temporary_same_state(base2) && index1.temporary_same_state(index2),
            (Expr::ArrayAccess { .. }, _) => false,
            (
                Expr::FieldAccess {
                    base: base1,
                    field: field1,
                    ..
                },
                Expr::FieldAccess {
                    base: base2,
                    field: field2,
                    ..
                },
            ) => base1.temporary_same_state(base2) && field1 == field2,
            (Expr::FieldAccess { .. }, _) => false,
            (
                Expr::BinaryOperation {
                    left,
                    operator,
                    right,
                    ..
                },
                Expr::BinaryOperation {
                    left: left2,
                    operator: operator2,
                    right: right2,
                    ..
                },
            ) => {
                left.temporary_same_state(left2)
                    && operator == operator2
                    && right.temporary_same_state(right2)
            }
            (Expr::BinaryOperation { .. }, _) => false,
            (
                Expr::UnaryOperation { operator, expr, .. },
                Expr::UnaryOperation {
                    operator: operator2,
                    expr: expr2,
                    ..
                },
            ) => operator == operator2 && expr.temporary_same_state(expr2),
            (Expr::UnaryOperation { .. }, _) => false,
        }
    }
}

impl<T: Clone> Expr<T> {
    pub fn free_vars(&self) -> HashSet<Name> {
        match self {
            Expr::Atom(_) => HashSet::new(),
            Expr::Block(block, _) => block.free_vars(),
            Expr::List(items, _) => items.iter().flat_map(|item| item.free_vars()).collect(),
            Expr::Map(entries, _) => entries
                .iter()
                .flat_map(|(_, value)| value.free_vars())
                .collect(),
            Expr::ClassConstructor { fields, spread, .. } => {
                let mut field_vars = fields
                    .iter()
                    .flat_map(|(_, value)| value.free_vars())
                    .collect::<HashSet<_>>();
                if let Some(spread) = spread {
                    field_vars.extend(spread.free_vars());
                }
                field_vars
            }
            Expr::Builtin(_, _) => HashSet::new(),
            Expr::FreeVar(name, _) => HashSet::from([name.clone()]),
            Expr::BoundVar(_, _) => HashSet::new(),
            Expr::Function(_, body, meta) => {
                Expr::Block(Box::new(Arc::unwrap_or_clone(body.clone())), meta.clone()).free_vars()
            }
            Expr::Call { func, args, .. } => {
                let mut free_vars = func.free_vars();
                free_vars.extend(args.iter().flat_map(|arg| arg.free_vars()));
                free_vars
            }
            Expr::If(cond, then, else_, _) => {
                let mut free_vars = cond.free_vars();
                free_vars.extend(then.free_vars());
                if let Some(else_) = else_ {
                    free_vars.extend(else_.free_vars());
                }
                free_vars
            }
            Expr::ForLoop {
                item,
                iterable,
                body,
                ..
            } => {
                let mut free_vars = iterable.free_vars();
                free_vars.extend(body.free_vars());
                free_vars.insert(item.clone());
                free_vars
            }
            Expr::ArrayAccess { base, index, .. } => {
                let mut free_vars = base.free_vars();
                free_vars.extend(index.free_vars());
                free_vars
            }
            Expr::FieldAccess { base, .. } => base.free_vars(),
            Expr::BinaryOperation { left, right, .. } => {
                let mut free_vars = left.free_vars();
                free_vars.extend(right.free_vars());
                free_vars
            }
            Expr::UnaryOperation { expr, .. } => expr.free_vars(),
        }
    }
}

/// Special methods for Exprs parameterized by the ExprMetadata type.
impl Expr<ExprMetadata> {
    /// Attempt to smoosh an expression that has been deeply evaluated into a BamlValue.
    /// If it encounters any non-evaluated sub-expressions, it returns None.
    pub fn as_atom(&self) -> Option<BamlValueWithMeta<ExprMetadata>> {
        match self {
            Expr::Atom(atom) => Some(atom.clone()),
            Expr::List(items, meta) => {
                let atom_items = items
                    .iter()
                    .map(|item| item.as_atom())
                    .collect::<Option<Vec<_>>>()?;
                Some(BamlValueWithMeta::List(atom_items, meta.clone()))
            }
            Expr::Map(entries, meta) => {
                let atom_entries = entries
                    .iter()
                    .map(|(key, value)| {
                        let atom = value.as_atom()?;
                        Some((key.clone(), atom))
                    })
                    .collect::<Option<BamlMap<String, BamlValueWithMeta<ExprMetadata>>>>()?;
                Some(BamlValueWithMeta::Map(atom_entries, meta.clone()))
            }
            // A class constructor may not be evaluated into an atom if it still contains a spread.
            Expr::ClassConstructor {
                name,
                fields,
                spread,
                meta,
            } => {
                if spread.is_some() {
                    None
                } else {
                    let atom_entries = fields
                        .iter()
                        .map(|(key, value)| {
                            let atom = value.as_atom()?;
                            Some((key.clone(), atom))
                        })
                        .collect::<Option<BamlMap<String, BamlValueWithMeta<ExprMetadata>>>>()?;
                    Some(BamlValueWithMeta::Class(
                        name.clone(),
                        atom_entries,
                        meta.clone(),
                    ))
                }
            }
            _ => None,
        }
    }

    pub fn span(&self) -> &Span {
        &self.meta().0
    }

    pub fn fresh_names(&self, arity: usize) -> Vec<Name> {
        let free_vars = self.free_vars();
        let mut i = 0;
        let mut names = Vec::new();
        while names.len() < arity {
            let candidate = format!("x_{i}");
            if !free_vars.contains(&candidate) {
                names.push(candidate);
            }
            i += 1;
        }
        names
    }

    pub fn fresh_name(&self) -> Name {
        self.fresh_names(1)[0].clone()
    }
}

impl<T: Clone + std::fmt::Debug> Expr<T> {
    /// Opens a term by replacing the bound variable with index k by a free variable.
    /// This operation is used when going under a binder.
    pub fn open(&self, target: &VarIndex, new_name: &str) -> Expr<T> {
        match self {
            Expr::Atom(v) => Expr::Atom(v.clone()),
            Expr::Block(block, m) => Expr::Block(Box::new(block.open(target, new_name)), m.clone()),
            Expr::List(items, m) => Expr::List(
                items.iter().map(|e| e.open(target, new_name)).collect(),
                m.clone(),
            ),
            Expr::Map(entries, m) => Expr::Map(
                entries
                    .iter()
                    .map(|(key, val)| (key.clone(), val.open(target, new_name)))
                    .collect(),
                m.clone(),
            ),
            Expr::ClassConstructor {
                name: class_name,
                fields,
                spread,
                meta: m,
            } => Expr::ClassConstructor {
                name: class_name.clone(),
                fields: fields
                    .iter()
                    .map(|(key, val)| (key.clone(), val.open(target, new_name)))
                    .collect(),
                spread: spread.as_ref().map(|s| Box::new(s.open(target, new_name))),
                meta: m.clone(),
            },
            Expr::FreeVar(n, m) => Expr::FreeVar(n.clone(), m.clone()),
            Expr::BoundVar(i, m) => {
                if i == target {
                    Expr::FreeVar(new_name.to_string(), m.clone())
                } else {
                    Expr::BoundVar(i.clone(), m.clone())
                }
            }
            Expr::Function(arity, body, m) => Expr::Function(
                *arity,
                Arc::new(body.open(&target.deeper(), new_name)),
                m.clone(),
            ),
            Expr::Call {
                func,
                args,
                meta,
                type_args,
            } => Expr::Call {
                func: Arc::new(func.open(target, new_name)),
                args: args.iter().map(|arg| arg.open(target, new_name)).collect(),
                type_args: type_args.clone(),
                meta: meta.clone(),
            },
            Expr::Builtin(builtin, m) => Expr::Builtin(builtin.clone(), m.clone()),
            Expr::If(cond, then, else_, m) => Expr::If(
                Arc::new(cond.open(target, new_name)),
                Arc::new(then.open(target, new_name)),
                else_.as_ref().map(|e| Arc::new(e.open(target, new_name))),
                m.clone(),
            ),
            Expr::ForLoop {
                item,
                iterable,
                body,
                meta,
            } => Expr::ForLoop {
                item: item.clone(),
                iterable: Arc::new(iterable.open(target, new_name)),
                body: Arc::new(body.open(target, new_name)),
                meta: meta.clone(),
            },
            Expr::ArrayAccess { base, index, meta } => Expr::ArrayAccess {
                base: Arc::new(base.open(target, new_name)),
                index: Arc::new(index.open(target, new_name)),
                meta: meta.clone(),
            },
            Expr::FieldAccess { base, field, meta } => Expr::FieldAccess {
                base: Arc::new(base.open(target, new_name)),
                field: field.clone(),
                meta: meta.clone(),
            },
            Expr::BinaryOperation {
                left,
                operator,
                right,
                meta,
            } => Expr::BinaryOperation {
                left: Arc::new(left.open(target, new_name)),
                operator: *operator,
                right: Arc::new(right.open(target, new_name)),
                meta: meta.clone(),
            },
            Expr::UnaryOperation {
                expr,
                operator,
                meta,
            } => Expr::UnaryOperation {
                operator: *operator,
                expr: Arc::new(expr.open(target, new_name)),
                meta: meta.clone(),
            },
        }
    }

    /// Closes a term by replacing the free variable with name by a bound variable with index k.
    /// This is the inverse operation of open.
    pub fn close(&self, new_index: &VarIndex, target: &str) -> Expr<T>
    where
        T: Clone + std::fmt::Debug,
    {
        match self {
            Expr::Atom(v) => Expr::Atom(v.clone()),
            Expr::Block(block, m) => {
                Expr::Block(Box::new(block.close(new_index, target)), m.clone())
            }
            Expr::List(items, m) => Expr::List(
                items.iter().map(|e| e.close(new_index, target)).collect(),
                m.clone(),
            ),
            Expr::Map(entries, m) => Expr::Map(
                entries
                    .iter()
                    .map(|(key, val)| (key.clone(), val.close(new_index, target)))
                    .collect(),
                m.clone(),
            ),
            Expr::ClassConstructor {
                name: class_name,
                fields,
                spread,
                meta: m,
            } => Expr::ClassConstructor {
                name: class_name.clone(),
                fields: fields
                    .iter()
                    .map(|(key, val)| (key.clone(), val.close(new_index, target)))
                    .collect(),
                spread: spread
                    .as_ref()
                    .map(|s| Box::new(s.close(new_index, target))),
                meta: m.clone(),
            },
            Expr::FreeVar(n, m) => {
                if n == target {
                    Expr::BoundVar(new_index.clone(), m.clone())
                } else {
                    Expr::FreeVar(n.clone(), m.clone())
                }
            }
            Expr::BoundVar(i, m) => Expr::BoundVar(i.clone(), m.clone()),
            Expr::Function(arity, body, m) => Expr::Function(
                *arity,
                Arc::new(body.close(&new_index.deeper(), target)),
                m.clone(),
            ),
            Expr::Call {
                func,
                args,
                meta,
                type_args,
            } => Expr::Call {
                func: Arc::new(func.close(new_index, target)),
                args: args
                    .iter()
                    .map(|arg| arg.close(new_index, target))
                    .collect(),
                type_args: type_args.clone(),
                meta: meta.clone(),
            },
            Expr::Builtin(builtin, m) => Expr::Builtin(builtin.clone(), m.clone()),
            Expr::If(cond, then, else_, m) => Expr::If(
                Arc::new(cond.close(new_index, target)),
                Arc::new(then.close(new_index, target)),
                else_.as_ref().map(|e| Arc::new(e.close(new_index, target))),
                m.clone(),
            ),
            Expr::ForLoop {
                item,
                iterable,
                body,
                meta,
            } => Expr::ForLoop {
                item: item.clone(),
                iterable: Arc::new(iterable.close(new_index, target)),
                body: Arc::new(body.close(new_index, target)),
                meta: meta.clone(),
            },
            Expr::ArrayAccess { base, index, meta } => Expr::ArrayAccess {
                base: Arc::new(base.close(new_index, target)),
                index: Arc::new(index.close(new_index, target)),
                meta: meta.clone(),
            },
            Expr::FieldAccess { base, field, meta } => Expr::FieldAccess {
                base: Arc::new(base.close(new_index, target)),
                field: field.clone(),
                meta: meta.clone(),
            },
            Expr::BinaryOperation {
                left,
                operator,
                right,
                meta,
            } => Expr::BinaryOperation {
                left: Arc::new(left.close(new_index, target)),
                operator: *operator,
                right: Arc::new(right.close(new_index, target)),
                meta: meta.clone(),
            },
            Expr::UnaryOperation {
                expr,
                operator,
                meta,
            } => Expr::UnaryOperation {
                operator: *operator,
                expr: Arc::new(expr.close(new_index, target)),
                meta: meta.clone(),
            },
        }
    }
}

/// An iterator over the sub-expressions of an expression.
impl<T: Clone> IntoIterator for Expr<T> {
    type Item = Expr<T>;
    type IntoIter = ExprIterator<T>;

    fn into_iter(self) -> Self::IntoIter {
        ExprIterator::new(self)
    }
}

/// An iterator over the sub-expressions of an expression.
pub struct ExprIterator<T> {
    pub stack: VecDeque<Expr<T>>,
}

impl<T> ExprIterator<T> {
    fn new(root: Expr<T>) -> Self {
        let mut stack = VecDeque::new();
        stack.push_back(root);
        Self { stack }
    }
}

impl<T: Clone> Iterator for ExprIterator<T> {
    type Item = Expr<T>;

    fn next(&mut self) -> Option<Self::Item> {
        let expr = self.stack.pop_back()?;

        // For exprs with sub-exprs, push the sub-exprs onto the stack.
        match expr.clone() {
            Expr::Atom(_) => {}
            Expr::Block(block, meta) => {
                self.stack
                    .push_back(Expr::Block(Box::new(*block.clone()), meta));
            }
            Expr::List(items, _) => {
                for item in items.into_iter() {
                    self.stack.push_back(item);
                }
            }
            Expr::Map(entries, _) => {
                for (_, value) in entries.into_iter() {
                    self.stack.push_back(value);
                }
            }
            Expr::ClassConstructor { fields, spread, .. } => {
                for (_, value) in fields.into_iter() {
                    self.stack.push_back(value);
                }
                if let Some(spread) = spread {
                    self.stack.push_back(*spread);
                }
            }
            Expr::FreeVar(_, _) => {}
            Expr::BoundVar(_, _) => {}
            Expr::Function(_, body, meta) => {
                self.stack.push_back(Expr::Block(
                    Box::new(Arc::unwrap_or_clone(body)),
                    meta.clone(),
                ));
            }
            Expr::Call { func, args, .. } => {
                self.stack.push_back(Arc::unwrap_or_clone(func));
                args.into_iter().for_each(|arg| self.stack.push_back(arg))
            }
            Expr::If(cond, then, else_, _) => {
                self.stack.push_back(Arc::unwrap_or_clone(cond));
                self.stack.push_back(Arc::unwrap_or_clone(then));
                if let Some(else_) = else_ {
                    self.stack.push_back(Arc::unwrap_or_clone(else_));
                }
            }
            Expr::ForLoop { iterable, body, .. } => {
                self.stack.push_back(Arc::unwrap_or_clone(iterable));
                self.stack.push_back(Arc::unwrap_or_clone(body));
            }
            Expr::ArrayAccess { base, index, .. } => {
                self.stack.push_back(Arc::unwrap_or_clone(base));
                self.stack.push_back(Arc::unwrap_or_clone(index));
            }
            Expr::FieldAccess { base, .. } => {
                self.stack.push_back(Arc::unwrap_or_clone(base));
            }
            Expr::Builtin(_, _) => {}
            Expr::BinaryOperation { left, right, .. } => {
                self.stack.push_back(Arc::unwrap_or_clone(left));
                self.stack.push_back(Arc::unwrap_or_clone(right));
            }
            Expr::UnaryOperation { expr, .. } => {
                self.stack.push_back(Arc::unwrap_or_clone(expr));
            }
        }

        Some(expr.clone())
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Variable {
    Bound(VarIndex),
    Free(Name),
}

/// A single unit of execution within a block.
#[derive(Clone, Debug)]
pub enum Statement<T> {
    /// Assign an immutable variable.
    Let {
        name: String,
        value: Expr<T>,
        span: Span,
    },
    /// Declare a (mutable) reference.
    /// There is no span because it is never present in the source AST.
    /// This is a desugaring from `if` expressions.
    Declare { name: String, span: Span },
    /// Assign a mutable variable.
    Assign { name: String, value: Expr<T> },
    /// Declare and assign a mutable reference in one statement.
    DeclareAndAssign {
        name: String,
        value: Expr<T>,
        span: Span,
    },
    /// Return from a function.
    FunctionReturn { expr: Expr<T>, span: Span },
    /// Evaluate an expression as the final value of a block (without returning from function).
    Expression { expr: Expr<T>, span: Span },
    While {
        condition: Box<Expr<T>>,
        block: Block<T>,
        span: Span,
    },
    ForLoop {
        identifier: String,
        iterator: Box<Expr<T>>,
        block: Block<T>,
        span: Span,
    },
}

impl<T: Clone> Statement<T> {
    pub fn dump_str(&self) -> String
    where
        T: std::fmt::Debug,
    {
        match self {
            Statement::Let {
                name,
                value,
                span: _,
            } => {
                format!("Let {} = {}", name, value.dump_str())
            }
            Statement::Declare { name, span: _ } => format!("var {name}"),
            Statement::Assign { name, value } => format!("{} <- {}", name, value.dump_str()),
            Statement::DeclareAndAssign {
                name,
                value,
                span: _,
            } => {
                format!("var {} <- {}", name, value.dump_str())
            }
            Statement::FunctionReturn { expr, span: _ } => {
                format!("return {}", expr.dump_str())
            }
            Statement::Expression { expr, span: _ } => expr.dump_str().to_string(),
            Statement::While {
                condition,
                block,
                span: _,
            } => {
                format!("while {} {{ {} }}", condition.dump_str(), block.dump_str())
            }
            Statement::ForLoop {
                identifier,
                iterator,
                block,
                span: _,
            } => {
                format!(
                    "for {} in {} {{ {} }}",
                    identifier,
                    iterator.dump_str(),
                    block.dump_str()
                )
            }
        }
    }

    pub fn free_vars(&self) -> HashSet<Name>
    where
        T: Clone,
    {
        match self {
            Statement::Let {
                name,
                value,
                span: _,
            } => value.free_vars(),
            Statement::Declare { name, span: _ } => HashSet::new(),
            Statement::Assign { name, value } => value.free_vars(),
            Statement::DeclareAndAssign {
                name,
                value,
                span: _,
            } => value.free_vars(),
            Statement::FunctionReturn { expr, span: _ } => expr.free_vars(),
            Statement::Expression { expr, span: _ } => expr.free_vars(),
            Statement::While {
                condition,
                block,
                span: _,
            } => {
                let mut free_vars = condition.free_vars();
                free_vars.extend(block.free_vars());
                free_vars
            }
            Statement::ForLoop {
                identifier,
                iterator,
                block,
                span: _,
            } => {
                let mut free_vars = iterator.free_vars();
                free_vars.extend(block.free_vars());
                free_vars
            }
        }
    }

    pub fn open(&self, target: &VarIndex, new_name: &str) -> Statement<T>
    where
        T: Clone + std::fmt::Debug,
    {
        match self {
            Statement::Let { name, value, span } => Statement::Let {
                name: name.clone(),
                value: value.open(target, new_name),
                span: span.clone(),
            },
            Statement::Declare { name, span } => Statement::Declare {
                name: name.clone(),
                span: span.clone(),
            },
            Statement::Assign { name, value } => Statement::Assign {
                name: name.clone(),
                value: value.open(target, new_name),
            },
            Statement::DeclareAndAssign { name, value, span } => Statement::DeclareAndAssign {
                name: name.clone(),
                value: value.open(target, new_name),
                span: span.clone(),
            },
            Statement::FunctionReturn { expr, span } => Statement::FunctionReturn {
                expr: expr.open(target, new_name),
                span: span.clone(),
            },
            Statement::Expression { expr, span } => Statement::Expression {
                expr: expr.open(target, new_name),
                span: span.clone(),
            },
            Statement::While {
                condition,
                block,
                span,
            } => Statement::While {
                condition: Box::new(condition.open(target, new_name)),
                block: block.open(target, new_name),
                span: span.clone(),
            },
            Statement::ForLoop {
                identifier,
                iterator,
                block,
                span,
            } => Statement::ForLoop {
                identifier: identifier.clone(),
                iterator: Box::new(iterator.open(target, new_name)),
                block: block.open(target, new_name),
                span: span.clone(),
            },
        }
    }

    pub fn close(&self, new_index: &VarIndex, target: &str) -> Statement<T>
    where
        T: Clone + std::fmt::Debug,
    {
        match self {
            Statement::Let { name, value, span } => Statement::Let {
                name: name.clone(),
                value: value.close(new_index, target),
                span: span.clone(),
            },
            Statement::Declare { name, span } => Statement::Declare {
                name: name.clone(),
                span: span.clone(),
            },
            Statement::Assign { name, value } => Statement::Assign {
                name: name.clone(),
                value: value.close(new_index, target),
            },
            Statement::DeclareAndAssign { name, value, span } => Statement::DeclareAndAssign {
                name: name.clone(),
                value: value.close(new_index, target),
                span: span.clone(),
            },
            Statement::FunctionReturn { expr, span } => Statement::FunctionReturn {
                expr: expr.close(new_index, target),
                span: span.clone(),
            },
            Statement::Expression { expr, span } => Statement::Expression {
                expr: expr.close(new_index, target),
                span: span.clone(),
            },
            Statement::While {
                condition,
                block,
                span,
            } => Statement::While {
                condition: Box::new(condition.close(new_index, target)),
                block: block.close(new_index, target),
                span: span.clone(),
            },
            Statement::ForLoop {
                identifier,
                iterator,
                block,
                span,
            } => Statement::ForLoop {
                identifier: identifier.clone(),
                iterator: Box::new(iterator.close(new_index, target)),
                block: block.close(new_index, target),
                span: span.clone(),
            },
        }
    }

    pub fn temporary_same_state(&self, other: &Statement<T>) -> bool
    where
        T: Clone + std::fmt::Debug,
    {
        match (self, other) {
            (
                Statement::Let {
                    name,
                    value,
                    span: _,
                },
                Statement::Let {
                    name: _,
                    value: _,
                    span: _,
                },
            ) => value.temporary_same_state(value),
            (
                Statement::Let {
                    name: _,
                    value: _,
                    span: _,
                },
                _,
            ) => false,
            (Statement::Declare { name, span: _ }, Statement::Declare { name: _, span: _ }) => true,
            (Statement::Declare { name: _, span: _ }, _) => false,
            (Statement::Assign { name, value }, Statement::Assign { name: _, value: _ }) => {
                value.temporary_same_state(value)
            }
            (Statement::Assign { name: _, value: _ }, _) => false,
            (
                Statement::DeclareAndAssign {
                    name,
                    value,
                    span: _,
                },
                Statement::DeclareAndAssign {
                    name: _,
                    value: value2,
                    span: _,
                },
            ) => value.temporary_same_state(value2),
            (Statement::DeclareAndAssign { .. }, _) => false,
            (
                Statement::FunctionReturn { expr, span: _ },
                Statement::FunctionReturn { expr: _, span: _ },
            ) => expr.temporary_same_state(expr),
            (Statement::FunctionReturn { .. }, _) => false,
            (
                Statement::Expression { expr, span: _ },
                Statement::Expression { expr: _, span: _ },
            ) => expr.temporary_same_state(expr),
            (Statement::Expression { .. }, _) => false,
            (
                Statement::While {
                    condition,
                    block,
                    span: _,
                },
                Statement::While {
                    condition: _,
                    block: block2,
                    span: _,
                },
            ) => condition.temporary_same_state(condition) && block.temporary_same_state(block2),
            (Statement::While { .. }, _) => false,
            (
                Statement::ForLoop {
                    identifier: _,
                    iterator,
                    block,
                    span: _,
                },
                Statement::ForLoop {
                    identifier: _,
                    iterator: iterator2,
                    block: block2,
                    span: _,
                },
            ) => iterator.temporary_same_state(iterator2) && block.temporary_same_state(block2),
            (Statement::ForLoop { .. }, _) => false,
        }
    }
}
