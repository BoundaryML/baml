// use moniker::{Binder, BoundTerm, Scope, Var};
use std::sync::Arc;

use crate::{BamlMap, BamlValueWithMeta, FieldType};
use itertools::intersperse;
use internal_baml_diagnostics::Span;

pub type Name = String;

/// A BAML expression term.
/// T is the type of the BamlValue metadata.
/// U is the type of arbitrary metadata, for example types.
// #[derive(Debug, Clone, BoundTerm)]
#[derive(Debug, Clone)]
pub enum Expr<T> {
    Atom(BamlValueWithMeta<T>),
    List(Vec<Expr<T>>, T),
    Map(BamlMap<String, Expr<T>>, T),
    Class(String, BamlMap<String, Expr<T>>, T),

    LLMFunction(Name, Vec<Name>, T),
    Var(Name, T),
    Lambda(Vec<Name>, Arc<Expr<T>>, T),
    App(Arc<Expr<T>>, Arc<Expr<T>>, T),
    Let(Name, Arc<Expr<T>>, Arc<Expr<T>>, T), // let name = expr in body
    ArgsTuple(Vec<Expr<T>>, T),
}

/// The metadata used during parsing, typechecking and evaluation of BAML expressions.
pub type ExprMetadata = (Span, Option<ExprType>);

impl <T: Clone + std::fmt::Debug> Expr<T> {
    pub fn meta(&self) -> &T {
        match self {
            Expr::Atom(baml_value) => baml_value.meta(),
            Expr::List(_, meta) => meta,
            Expr::Map(_, meta) => meta,
            Expr::Class(_, _, meta) => meta,
            Expr::LLMFunction(_, _, meta) => meta,
            Expr::Var(_, meta) => meta,
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
            Expr::Class(_, _, meta) => meta,
            Expr::LLMFunction(_, _, meta) => meta,
            Expr::Var(_, meta) => meta,
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
            Expr::Class(_, _, meta) => meta,
            Expr::LLMFunction(_, _, meta) => meta,
            Expr::Var(_, meta) => meta,
            Expr::Lambda(_, _, meta) => meta,
            Expr::App(_, _, meta) => meta,
            Expr::ArgsTuple(_, meta) => meta,
            Expr::Let(_, _, _, meta) => meta,
        }
    }
}

impl <T: Clone + std::fmt::Debug> Expr<T> {

    /// A very rough pretty-printer for debugging expressions.
    pub fn dump_str(&self) -> String {
        match self {
            Expr::Atom(atom) => atom.clone().value().to_string(),
            Expr::LLMFunction(name, _, _) => name.clone(),
            Expr::Var(name, _) => name.clone(),
            Expr::Lambda(args, body, _) => format!("\\{:?} -> {}", args, body.dump_str()),
            Expr::App(func, args, _) => {
                let args_str = match args.as_ref() {
                    Expr::ArgsTuple(args, _) => args.iter().map(|arg| arg.dump_str()).collect::<Vec<_>>().join(", "),
                    _ => format!("(NON_ARGS_TUPLE {})", args.dump_str()),
                };
                let func_str = match func.as_ref() {
                    Expr::LLMFunction(name, _, _) => name.clone(),
                    Expr::Var(name, _) => name.clone(),
                    _ => format!("({})", func.dump_str()),
                };
                format!("{}({})", func_str, args_str)
            },
            Expr::Let(name, expr, body, _) => format!("Let {} = {} in {}", name, expr.dump_str(), body.dump_str()),
            Expr::ArgsTuple(args, _) => format!("ArgsTuple({:?})", args.iter().map(|arg| arg.dump_str()).collect::<Vec<_>>()),
            Expr::List(items, _) => {
                let items = intersperse(items.iter().map(|item| item.dump_str()).collect::<Vec<_>>(), ", ");
                format!("[{}]", items)
            }
            Expr::Map(entries, _) => {
                let entries = entries.iter().map(|(key, value)| format!("{}: {}", key, value.dump_str())).collect::<Vec<_>>().join(", ");
                format!("{{{}}}", entries)
            }
            Expr::Class(name, entries, _) => {
                let entries = entries.iter().map(|(key, value)| format!("{}: {}", key, value.dump_str())).collect::<Vec<_>>().join(", ");
                format!("Class({} {{ {}}}", name, entries)
            }
            
        }
    }

    /// This quick hack of a function checks whether two expressions are
    /// equal in terms of reduction state. This test is used to detect
    /// if the evaluation stepper is stuck.
    pub fn temporary_same_state(&self, other: &Expr<T,U>) -> bool {
        match (self, other) {
            (Expr::Atom(a1), Expr::Atom(a2)) => a1.clone().value() == a2.clone().value(),
            (Expr::LLMFunction(n1, _, _), Expr::LLMFunction(n2, _, _)) => n1 == n2,
            (Expr::Var(n1, _), Expr::Var(n2, _)) => n1 == n2,
            (Expr::Lambda(args1, body1, _), Expr::Lambda(args2, body2, _)) => {
                args1 == args2 && body1.temporary_same_state(body2)
            }
            (Expr::App(f1, x1, _), Expr::App(f2, x2, _)) => {
                f1.temporary_same_state(f2) && x1.temporary_same_state(x2)
            }
            (Expr::Let(n1, e1, b1, _), Expr::Let(n2, e2, b2, _)) => {
                n1 == n2 && e1.temporary_same_state(e2) && b1.temporary_same_state(b2)
            }
            (Expr::ArgsTuple(args1, _), Expr::ArgsTuple(args2, _)) => {
                args1.iter().zip(args2.iter()).all(|(a1, a2)| a1.temporary_same_state(a2))
            }
            (Expr::Class(n1, e1, _), Expr::Class(n2, e2, _)) => {
                n1 == n2 && e1.temporary_same_state(e2)
            }
            _ => false,
        }
    }
}

/// Spetial methods for Exprs parameterized by the ExprMetadata type.
impl Expr<ExprMetadata> {

    pub fn as_atom(&self) -> Option<&BamlValueWithMeta<T>> {
        match self {
            Expr::Atom(atom) => Some(atom),
            Expr::List(items, _) => {
                let atom_items = items.iter().map(|item| item.as_atom()).collect::<Option<Vec<_>>>()?;
                Some(BamlValueWithMeta::List(atom_items, ()))
            }
            _ => None,
        }
    }
}


#[derive(Debug, Clone)]
pub enum ExprType {
    Atom(FieldType),
    Arrow(Box<Arrow>),
}

#[derive(Debug, Clone)]
pub struct Arrow {
    pub param_types: Vec<ExprType>,
    pub body_type: ExprType,
}

impl ExprType {
    pub fn dump_str(&self) -> String {
        match self {
            ExprType::Atom(ft) => ft.to_string(),
            ExprType::Arrow(arrow) => {
                let param_types_str = arrow.param_types.iter().map(|t| t.dump_str()).collect::<Vec<_>>().join(", ");
                format!("({}) -> {}", param_types_str, arrow.body_type.dump_str())
            },
        }
    }
}
