//! Per-body type references: every type expression written inside a body,
//! lowered once into a span-free [`TypeRefStore`] - the body-side analog of
//! the per-item stores in ppir's item data, and the rust-analyzer shape
//! (bodies own their type refs; consumers see ids, never syntax).
//!
//! The collection walks the body's arenas in allocation order, so ids are a
//! pure function of body shape - stable under whitespace edits (`TypeExpr`
//! equality already ignores spans; this makes the span-freedom structural).
//! Spans stay in the lockstep [`TypeRefSourceMap`] for future diagnostics.
//!
//! Collected today: pattern type ascriptions (`let x: T`, type patterns in
//! match arms), array-pattern ascriptions, explicit expression-position type
//! args (`f<int>(..)`, `Box<int> { .. }` turbofish), `.as<T>` upcast
//! targets, and lambda signature slots. Class-destructure generic args join
//! when pattern inference needs them.

use baml_base::Name;
use baml_compiler2_ast::{Expr, ExprBody, ExprId, PatId, Pattern};
use rustc_hash::FxHashMap;

use crate::type_ref::{TypeRefBuilder, TypeRefId, TypeRefSourceMap, TypeRefStore};

/// All type references written in one body, keyed by the nodes that carry
/// them. A missing key means the position had no written annotation.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct BodyTypeRefs {
    pub store: TypeRefStore,
    /// `Pattern::Type` nodes: `let x: T` ascriptions (as the bind's
    /// sub-pattern) and bare type patterns in match arms.
    pub pattern_types: FxHashMap<PatId, TypeRefId>,
    /// Array-pattern `[...]: T` ascriptions.
    pub array_ascriptions: FxHashMap<PatId, TypeRefId>,
    /// Class-destructure generic args (`Box<int> { v }` patterns), in
    /// written order. Never empty when present.
    pub pattern_class_args: FxHashMap<PatId, Box<[TypeRefId]>>,
    /// Destructure-head associated bindings (`Source<Item = int> { v }`
    /// patterns), in written order. Never empty when present.
    pub pattern_assoc_bindings: FxHashMap<PatId, Box<[(Name, TypeRefId)]>>,
    /// Explicit expression-position type arguments (`Call`, `GenericApply`,
    /// and `Object` constructors), in written order. Never empty when
    /// present.
    pub expr_type_args: FxHashMap<ExprId, Box<[TypeRefId]>>,
    /// `.as<T>` upcast targets.
    pub upcast_targets: FxHashMap<ExprId, TypeRefId>,
    /// Lambda signature slots, keyed by the lambda expression.
    pub lambda_signatures: FxHashMap<ExprId, LambdaTypeRefs>,
}

/// A lambda's written signature slots (each optional: lambdas may omit any
/// of them and rely on expectation-driven inference).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LambdaTypeRefs {
    /// Per declared parameter, in order.
    pub params: Box<[Option<TypeRefId>]>,
    pub return_type: Option<TypeRefId>,
    pub throws: Option<TypeRefId>,
}

/// Lowers every type expression in `body` into one store. Pure; ppir wraps
/// this in salsa queries over its canonical bodies.
pub fn collect_body_type_refs(body: &ExprBody) -> (BodyTypeRefs, TypeRefSourceMap) {
    let mut builder = TypeRefBuilder::new();
    let mut refs = BodyTypeRefs::default();

    for (pat_id, pattern) in body.patterns.iter() {
        match pattern {
            Pattern::Type(type_expr) => {
                refs.pattern_types.insert(pat_id, builder.lower(type_expr));
            }
            Pattern::Array {
                ascription: Some(type_expr),
                ..
            } => {
                refs.array_ascriptions
                    .insert(pat_id, builder.lower(type_expr));
            }
            Pattern::Class {
                generic_args,
                associated_type_bindings,
                ..
            } if !generic_args.is_empty() || !associated_type_bindings.is_empty() => {
                if !generic_args.is_empty() {
                    refs.pattern_class_args.insert(
                        pat_id,
                        generic_args.iter().map(|arg| builder.lower(arg)).collect(),
                    );
                }
                if !associated_type_bindings.is_empty() {
                    refs.pattern_assoc_bindings.insert(
                        pat_id,
                        associated_type_bindings
                            .iter()
                            .map(|binding| (binding.name.clone(), builder.lower(&binding.ty)))
                            .collect(),
                    );
                }
            }
            _ => {}
        }
    }

    for (expr_id, expr) in body.exprs.iter() {
        match expr {
            Expr::Call { type_args, .. }
            | Expr::GenericApply { type_args, .. }
            | Expr::Object { type_args, .. }
                if !type_args.is_empty() =>
            {
                refs.expr_type_args.insert(
                    expr_id,
                    type_args.iter().map(|arg| builder.lower(arg)).collect(),
                );
            }
            Expr::Upcast { target, .. } => {
                refs.upcast_targets.insert(expr_id, builder.lower(target));
            }
            Expr::Lambda(def) => {
                refs.lambda_signatures.insert(
                    expr_id,
                    LambdaTypeRefs {
                        params: def
                            .params
                            .iter()
                            .map(|param| param.type_expr.as_ref().map(|ty| builder.lower(ty)))
                            .collect(),
                        return_type: def.return_type.as_ref().map(|ty| builder.lower(ty)),
                        throws: def.throws.as_ref().map(|ty| builder.lower(ty)),
                    },
                );
            }
            _ => {}
        }
    }

    let (store, source_map) = builder.finish();
    refs.store = store;
    (refs, source_map)
}
