// use moniker::{Binder, BoundTerm, Scope, Var};
use std::collections::HashSet;
use std::sync::Arc;

use crate::{field_type::FieldType, BamlMap, BamlValueWithMeta};
use internal_baml_diagnostics::Span;
use itertools::join;

/// A BAML expression term.
/// T is the type of the metadata.
#[derive(Debug, Clone)]
pub enum Expr<T> {
    Atom(BamlValueWithMeta<T>),
    List(Vec<Expr<T>>, T),
    Map(BamlMap<String, Expr<T>>, T),
    ClassConstructor {
        name: String,
        fields: BamlMap<String, Expr<T>>,
        spread: Option<Box<Expr<T>>>,
        meta: T,
    },

    LLMFunction(Name, Vec<Name>, T),
    // A free variable, not bound by a lambda.
    FreeVar(Name, T),
    // The DeBruijn index of a bound variable.
    BoundVar(VarIndex, T),
    Lambda(usize, Arc<Expr<T>>, T), // number of parameters, body, metadata
    App(Arc<Expr<T>>, Arc<Expr<T>>, T),
    Let(Name, Arc<Expr<T>>, Arc<Expr<T>>, T), // let name = expr in body
    ArgsTuple(Vec<Expr<T>>, T),
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
pub type ExprMetadata = (Span, Option<FieldType>);

impl<T: Clone + std::fmt::Debug> Expr<T> {
    pub fn meta(&self) -> &T {
        match self {
            Expr::Atom(baml_value) => baml_value.meta(),
            Expr::List(_, meta) => meta,
            Expr::Map(_, meta) => meta,
            Expr::ClassConstructor { meta, .. } => meta,
            Expr::LLMFunction(_, _, meta) => meta,
            Expr::BoundVar(_, meta) => meta,
            Expr::FreeVar(_, meta) => meta,
            Expr::Lambda(_, _, meta) => meta,
            Expr::App(_, _, meta) => meta,
            Expr::ArgsTuple(_, meta) => meta,
            Expr::Let(_, _, _, meta) => meta,
        }
    }

    pub fn meta_mut(&mut self) -> &mut T {
        match self {
            Expr::Atom(baml_value) => baml_value.meta_mut(),
            Expr::List(_, meta) => meta,
            Expr::Map(_, meta) => meta,
            Expr::ClassConstructor { meta, .. } => meta,
            Expr::LLMFunction(_, _, meta) => meta,
            Expr::BoundVar(_, meta) => meta,
            Expr::FreeVar(_, meta) => meta,
            Expr::Lambda(_, _, meta) => meta,
            Expr::App(_, _, meta) => meta,
            Expr::Let(_, _, _, meta) => meta,
            Expr::ArgsTuple(_, meta) => meta,
        }
    }

    pub fn into_meta(self) -> T {
        match self {
            Expr::Atom(baml_value) => baml_value.meta().clone(),
            Expr::List(_, meta) => meta,
            Expr::Map(_, meta) => meta,
            Expr::ClassConstructor { meta, .. } => meta,
            Expr::LLMFunction(_, _, meta) => meta,
            Expr::BoundVar(_, meta) => meta,
            Expr::FreeVar(_, meta) => meta,
            Expr::Lambda(_, _, meta) => meta,
            Expr::App(_, _, meta) => meta,
            Expr::ArgsTuple(_, meta) => meta,
            Expr::Let(_, _, _, meta) => meta,
        }
    }
}

impl<T: Clone + std::fmt::Debug> Expr<T> {
    /// A very rough pretty-printer for debugging expressions.
    pub fn dump_str(&self) -> String {
        match self {
            Expr::Atom(atom) => atom.clone().value().to_string(),
            Expr::LLMFunction(name, _, _) => name.clone(),
            Expr::BoundVar(ind, _) => ind.dump_str(),
            Expr::FreeVar(name, _) => name.clone(),
            Expr::Lambda(_, body, _) => format!("\\. -> {}", body.dump_str()),
            Expr::App(func, args, _) => {
                let args_str = match args.as_ref() {
                    Expr::ArgsTuple(args, _) => args
                        .iter()
                        .map(|arg| arg.dump_str())
                        .collect::<Vec<_>>()
                        .join(", "),
                    _ => format!("(NON_ARGS_TUPLE {})", args.dump_str()),
                };
                let func_str = match func.as_ref() {
                    Expr::LLMFunction(name, _, _) => name.clone(),
                    Expr::BoundVar(ind, _) => ind.dump_str(),
                    Expr::FreeVar(name, _) => name.clone(),
                    _ => format!("({})", func.dump_str()),
                };
                format!("{}({})", func_str, args_str)
            }
            Expr::Let(name, expr, body, _) => {
                format!("Let {} = {} in {}", name, expr.dump_str(), body.dump_str())
            }
            Expr::ArgsTuple(args, _) => format!(
                "ArgsTuple({:?})",
                args.iter().map(|arg| arg.dump_str()).collect::<Vec<_>>()
            ),
            Expr::List(items, _) => {
                let items = join(
                    items.iter().map(|item| item.dump_str()).collect::<Vec<_>>(),
                    ", ",
                );
                format!("[{}]", items)
            }
            Expr::Map(entries, _) => {
                let entries = entries
                    .iter()
                    .map(|(key, value)| format!("{}: {}", key, value.dump_str()))
                    .collect::<Vec<_>>()
                    .join(", ");
                format!("{{{}}}", entries)
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
                format!("Class({} {{ {}{} }}", name, fields, spread)
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

            (Expr::LLMFunction(n1, _, _), Expr::LLMFunction(n2, _, _)) => n1 == n2,
            (Expr::LLMFunction(_, _, _), _) => false,

            (Expr::BoundVar(n1, _), Expr::BoundVar(n2, _)) => n1 == n2,
            (Expr::BoundVar(_, _), _) => false,

            (Expr::FreeVar(n1, _), Expr::FreeVar(n2, _)) => n1 == n2,
            (Expr::FreeVar(_, _), _) => false,

            (Expr::Lambda(arity1, body1, _), Expr::Lambda(arity2, body2, _)) => {
                arity1 == arity2 && body1.temporary_same_state(body2)
            }
            (Expr::Lambda(_, _, _), _) => false,

            (Expr::App(f1, x1, _), Expr::App(f2, x2, _)) => {
                f1.temporary_same_state(f2) && x1.temporary_same_state(x2)
            }
            (Expr::App(_, _, _), _) => false,

            (Expr::Let(n1, e1, b1, _), Expr::Let(n2, e2, b2, _)) => {
                n1 == n2 && e1.temporary_same_state(e2) && b1.temporary_same_state(b2)
            }
            (Expr::Let(_, _, _, _), _) => false,

            (Expr::ArgsTuple(args1, _), Expr::ArgsTuple(args2, _)) => {
                args1.len() == args2.len()
                    && args1
                        .iter()
                        .zip(args2.iter())
                        .all(|(a1, a2)| a1.temporary_same_state(a2))
            }
            (Expr::ArgsTuple(_, _), _) => false,

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

    pub fn free_vars(&self) -> HashSet<Name> {
        match self {
            Expr::Atom(_) => HashSet::new(),
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
            Expr::LLMFunction(_, _, _) => HashSet::new(),
            Expr::FreeVar(name, _) => HashSet::from([name.clone()]),
            Expr::BoundVar(_, _) => HashSet::new(),
            Expr::Lambda(_, body, _) => body.free_vars(),
            Expr::App(func, args, _) => {
                let mut free_vars = func.free_vars();
                free_vars.extend(args.free_vars());
                free_vars
            }
            Expr::Let(_, expr, body, _) => {
                let mut free_vars = expr.free_vars();
                free_vars.extend(body.free_vars());
                free_vars
            }
            Expr::ArgsTuple(args, _) => args.iter().flat_map(|a| a.free_vars()).collect(),
        }
    }

    pub fn fresh_names(&self, arity: usize) -> Vec<Name> {
        let free_vars = self.free_vars();
        let mut i = 0;
        let mut names = Vec::new();
        while names.len() < arity {
            let candidate = format!("x_{}", i);
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

impl<T: Clone> Expr<T> {
    /// Opens a term by replacing the bound variable with index k by a free variable.
    /// This operation is used when going under a binder.
    pub fn open(&self, target: &VarIndex, new_name: &str) -> Expr<T> {
        match self {
            Expr::Atom(v) => Expr::Atom(v.clone()),
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
            Expr::LLMFunction(n, args, m) => Expr::LLMFunction(n.clone(), args.clone(), m.clone()),
            Expr::FreeVar(n, m) => Expr::FreeVar(n.clone(), m.clone()),
            Expr::BoundVar(i, m) => {
                if i == target {
                    Expr::FreeVar(new_name.to_string(), m.clone())
                } else {
                    Expr::BoundVar(i.clone(), m.clone())
                }
            }
            Expr::Lambda(arity, body, m) => Expr::Lambda(
                *arity,
                Arc::new(body.open(&target.deeper(), new_name)),
                m.clone(),
            ),
            Expr::App(f, x, m) => Expr::App(
                Arc::new(f.open(target, new_name)),
                Arc::new(x.open(target, new_name)),
                m.clone(),
            ),
            Expr::Let(n, e, body, m) => Expr::Let(
                n.clone(),
                Arc::new(e.open(target, new_name)),
                Arc::new(body.open(target, new_name)),
                m.clone(),
            ),
            Expr::ArgsTuple(args, m) => Expr::ArgsTuple(
                args.iter().map(|e| e.open(target, new_name)).collect(),
                m.clone(),
            ),
        }
    }

    /// Closes a term by replacing the free variable with name by a bound variable with index k.
    /// This is the inverse operation of open.
    pub fn close(&self, new_index: &VarIndex, target: &str) -> Expr<T> {
        match self {
            Expr::Atom(v) => Expr::Atom(v.clone()),
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
            Expr::LLMFunction(n, args, m) => Expr::LLMFunction(n.clone(), args.clone(), m.clone()),
            Expr::FreeVar(n, m) => {
                if n == target {
                    Expr::BoundVar(new_index.clone(), m.clone())
                } else {
                    Expr::FreeVar(n.clone(), m.clone())
                }
            }
            Expr::BoundVar(i, m) => Expr::BoundVar(i.clone(), m.clone()),
            Expr::Lambda(arity, body, m) => Expr::Lambda(
                *arity,
                Arc::new(body.close(&new_index.deeper(), target)),
                m.clone(),
            ),
            Expr::App(f, x, m) => Expr::App(
                Arc::new(f.close(new_index, target)),
                Arc::new(x.close(new_index, target)),
                m.clone(),
            ),
            Expr::Let(n, e, body, m) => Expr::Let(
                n.clone(),
                Arc::new(e.close(new_index, target)),
                Arc::new(body.close(new_index, target)),
                m.clone(),
            ),
            Expr::ArgsTuple(args, m) => Expr::ArgsTuple(
                args.iter().map(|e| e.close(new_index, target)).collect(),
                m.clone(),
            ),
        }
    }
}
