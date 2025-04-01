// use moniker::{Binder, BoundTerm, Scope, Var};
use std::sync::Arc;

use crate::{BamlMap, BamlValueWithMeta, field_type::FieldType};
use itertools::join;
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
    ClassConstructor{name: String, fields: BamlMap<String, Expr<T>>, spread: Option<Box<Expr<T>>>, meta: T},

    LLMFunction(Name, Vec<Name>, T),
    Var(Name, T),
    Lambda(Vec<Name>, Arc<Expr<T>>, T),
    App(Arc<Expr<T>>, Arc<Expr<T>>, T),
    Let(Name, Arc<Expr<T>>, Arc<Expr<T>>, T), // let name = expr in body
    ArgsTuple(Vec<Expr<T>>, T),
}

/// The metadata used during parsing, typechecking and evaluation of BAML expressions.
pub type ExprMetadata = (Span, Option<FieldType>);

impl <T: Clone + std::fmt::Debug> Expr<T> {
    pub fn meta(&self) -> &T {
        match self {
            Expr::Atom(baml_value) => baml_value.meta(),
            Expr::List(_, meta) => meta,
            Expr::Map(_, meta) => meta,
            Expr::ClassConstructor{meta, ..} => meta,
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
            Expr::ClassConstructor{ meta, ..} => meta,
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
            Expr::ClassConstructor{meta, ..} => meta,
            Expr::LLMFunction(_, _, meta) => meta,
            Expr::Var(_, meta) => meta,
            Expr::Lambda(_, _, meta) => meta,
            Expr::App(_, _, meta) => meta,
            Expr::ArgsTuple(_, meta) => meta,
            Expr::Let(_, _, _, meta) => meta,
        }
    }

    // pub fn assert_eq_up_to_span(&self, other: &Expr<T>) {
    //     match (self, other) {
    //         (Expr::Atom(a1), Expr::Atom(a2)) => assert_eq!(a1.clone().value(), a2.clone().value()),
    //         (Expr::Atom(_), _) => panic!("Atom mismatch"),

    //         (Expr::List(l1, _), Expr::List(l2, _)) => {
    //             assert_eq!(l1.len(), l2.len());
    //             l1.iter().zip(l2.iter()).for_each(|(e1, e2)| e1.assert_eq_up_to_span(e2));
    //         }
    //         (Expr::List(_, _), _) => panic!("List mismatch"),
    //         
    //         (Expr::ClassConstructor{name: n1, fields: f1, spread: s1, ..}, Expr::ClassConstructor{name: n2, fields: f2, spread: s2, ..}) => {
    //             assert_eq!(n1, n2);
    //             for ((k1,v1), (k2,v2)) in f1.iter().zip(f2.iter()) {
    //                 assert_eq!(k1, k2);
    //                 v1.assert_eq_up_to_span(v2);
    //             }
    //             if let (Some(s1), Some(s2)) = (s1, s2) {
    //                 s1.assert_eq_up_to_span(s2);
    //             } else if s1.is_some() || s2.is_some() {
    //                 panic!("Spread mismatch");
    //             }
    //         }
    //         (Expr::ClassConstructor{..}, _) => panic!("ClassConstructor mismatch"),

    //         (Expr::LLMFunction(n1, args1, _), Expr::LLMFunction(n2, args2, _)) => {
    //             assert_eq!(n1, n2);
    //             assert_eq!(args1, args2);
    //         }
    //         (Expr::LLMFunction(_, _, _), _) => panic!("LLMFunction mismatch"),

    //         (Expr::Var(n1, _), Expr::Var(n2, _)) => assert_eq!(n1, n2),
    //         (Expr::Var(_, _), _) => panic!("Var mismatch"),

    //         (Expr::Lambda(args1, body1, _), Expr::Lambda(args2, body2, _)) => {
    //             assert_eq!(args1, args2);
    //             body1.assert_eq_up_to_span(body2);
    //         }
    //         (Expr::Lambda(_, _, _), _) => panic!("Lambda mismatch"),

    //         (Expr::App(f1, x1, _), Expr::App(f2, x2, _)) => {
    //             f1.assert_eq_up_to_span(f2);
    //             x1.assert_eq_up_to_span(x2);
    //         }
    //         (Expr::App(_, _, _), _) => panic!("App mismatch"),

    //         (Expr::Let(n1, e1, b1, _), Expr::Let(n2, e2, b2, _)) => {
    //             assert_eq!(n1, n2);
    //             e1.assert_eq_up_to_span(e2);
    //             b1.assert_eq_up_to_span(b2);
    //         }
    //         (Expr::Let(_, _, _, _), _) => panic!("Let mismatch"),

    //         (Expr::ArgsTuple(args1, _), Expr::ArgsTuple(args2, _)) => {
    //             assert_eq!(args1.len(), args2.len());
    //             args1.iter().zip(args2.iter()).for_each(|(a1, a2)| a1.assert_eq_up_to_span(a2));
    //         }
    //         (Expr::ArgsTuple(_, _), _) => panic!("ArgsTuple mismatch"),

    //         (Expr::Map(e1, _), Expr::Map(e2, _)) => {
    //             assert_eq!(e1.len(), e2.len());
    //             e1.iter().zip(e2.iter()).for_each(|((_k1, v1), (_k2, v2))| v1.assert_eq_up_to_span(v2));
    //         }
    //         (Expr::Map(_, _), _) => panic!("Map mismatch"),
    //         
    //     }
    // }
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
                let items = join(items.iter().map(|item| item.dump_str()).collect::<Vec<_>>(), ", ");
                format!("[{}]", items)
            }
            Expr::Map(entries, _) => {
                let entries = entries.iter().map(|(key, value)| format!("{}: {}", key, value.dump_str())).collect::<Vec<_>>().join(", ");
                format!("{{{}}}", entries)
            }
            Expr::ClassConstructor{name, fields, spread, ..} => {
                let fields = fields.iter().map(|(key, value)| format!("{}: {}", key, value.dump_str())).collect::<Vec<_>>().join(", ");
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
            
            (Expr::Var(n1, _), Expr::Var(n2, _)) => n1 == n2,
            (Expr::Var(_, _), _) => false,

            (Expr::Lambda(args1, body1, _), Expr::Lambda(args2, body2, _)) => {
                args1 == args2 && body1.temporary_same_state(body2)
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
                args1.len() == args2.len() &&
                args1.iter().zip(args2.iter()).all(|(a1, a2)| a1.temporary_same_state(a2))
            }
            (Expr::ArgsTuple(_, _), _) => false,

            (Expr::ClassConstructor{name: n1, fields: e1, spread: s1, ..}, Expr::ClassConstructor{name: n2, fields: e2, spread: s2, ..}) => {
                n1 == n2 &&
                e1.len() == e2.len() &&
                e1.iter().zip(e2.iter()).all(|((_k1, v1), (_k2, v2))| v1.temporary_same_state(v2)) &&
                (match (s1, s2) {
                    (Some(s1), Some(s2)) => s1.temporary_same_state(s2),
                    (None, None) => true,
                    _ => false,
                })
            }
            (Expr::ClassConstructor{..}, _) => false,

            (Expr::Map(e1, _), Expr::Map(e2, _)) => {
                e1.len() == e2.len() &&
                e1.iter().zip(e2.iter()).all(|((_k1, v1), (_k2, v2))| v1.temporary_same_state(v2))
            }
            (Expr::Map(_, _), _) => false,

            (Expr::List(e1, _), Expr::List(e2, _)) => {
                e1.len() == e2.len() &&
                e1.iter().zip(e2.iter()).all(|(a1, a2)| a1.temporary_same_state(a2))
            }
            (Expr::List(_, _), _) => false,

        }
    }
}

/// Spetial methods for Exprs parameterized by the ExprMetadata type.
impl Expr<ExprMetadata> {

    /// Attemp to smoosh an expression that has been deeply evaluated into a BamlValue.
    /// If it encounteres any non-evaluated sub-expressions, it returns None.
    pub fn as_atom(&self) -> Option<BamlValueWithMeta<ExprMetadata>> {
        match self {
            Expr::Atom(atom) => Some(atom.clone()),
            Expr::List(items, meta) => {
                let atom_items = items.iter().map(|item| item.as_atom()).collect::<Option<Vec<_>>>()?;
                Some(BamlValueWithMeta::List(atom_items, meta.clone()))
            }
            Expr::Map(entries, meta) => {
                let atom_entries = entries.iter().map(|(key, value)| {
                    let atom = value.as_atom()?;
                    Some((key.clone(), atom))
                }).collect::<Option<BamlMap<String, BamlValueWithMeta<ExprMetadata>>>>()?;
                Some(BamlValueWithMeta::Map(atom_entries, meta.clone()))
            }
            // A class constructor may not be evaluated into an atom if it still contains a spread.
            Expr::ClassConstructor{name, fields, spread, meta} => {
                 if spread.is_some() {
                    None
                 } else {
                    let atom_entries = fields.iter().map(|(key, value)| {
                        let atom = value.as_atom()?;
                        Some((key.clone(), atom))
                    }).collect::<Option<BamlMap<String, BamlValueWithMeta<ExprMetadata>>>>()?;
                    Some(BamlValueWithMeta::Class(name.clone(), atom_entries, meta.clone()))
                 }
            }
            _ => None,
        }
    }
}


// #[derive(Debug, Clone)]
// pub enum ExprType {
//     Atom(FieldType),
//     Arrow(Box<Arrow>),
//     List(Box<ExprType>),
//     Map(Box<ExprType>),
//     Class(String, BamlMap<String, ExprType>),
// }
// 
// #[derive(Debug, Clone)]
// pub struct Arrow {
//     pub param_types: Vec<ExprType>,
//     pub body_type: ExprType,
// }
// 
// impl ExprType {
//     pub fn dump_str(&self) -> String {
//         match self {
//             ExprType::Atom(ft) => ft.to_string(),
//             ExprType::Arrow(arrow) => {
//                 let param_types_str = arrow.param_types.iter().map(|t| t.dump_str()).collect::<Vec<_>>().join(", ");
//                 format!("({}) -> {}", param_types_str, arrow.body_type.dump_str())
//             },
//         }
//     }
// }
// 