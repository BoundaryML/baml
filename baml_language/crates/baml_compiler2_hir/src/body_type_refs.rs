//! Per-body type references: every type expression written inside a body,
//! lowered once into a span-free [`TypeRefStore`] - the body-side analog of
//! the per-item stores in ppir's item data, and the rust-analyzer shape
//! (bodies own their type refs; consumers see ids, never syntax).
//!
//! The collection walks the body's arenas in allocation order, so ids are a
//! pure function of body shape - stable under whitespace edits (`TypeExpr`
//! equality already ignores spans; this makes the span-freedom structural).
//! Spans stay in the lockstep [`BodyTypeRefSourceMap`] for future diagnostics.
//!
//! Collected today: pattern type ascriptions (`let x: T`, type patterns in
//! match arms), array-pattern ascriptions, explicit expression-position type
//! args (`f<int>(..)`, `Box<int> { .. }` turbofish), `.as<T>` upcast targets,
//! match scrutinee annotations, lambda signature slots, and the static
//! right-hand side of a `type T = …` binding (a runtime `unreflect(expr)`
//! right-hand side is an ordinary body expression, not a type reference).
//! Class-destructure generic args join when pattern inference needs them.

use baml_base::Name;
use baml_compiler2_ast::{Expr, ExprBody, ExprId, PatId, Pattern, Stmt, StmtId, TypeBindingValue};
use rustc_hash::FxHashMap;

use crate::type_ref::{TypeRefBuilder, TypeRefId, TypeRefSourceMap, TypeRefStore};

/// Identity of a type reference in a body-owned arena.
///
/// This is intentionally distinct from declaration [`TypeRefId`]s. Converting
/// back to the raw arena index requires the owning [`BodyTypeRefs`], keeping
/// body-relative diagnostic anchors out of signature lowering.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct BodyTypeRefId(TypeRefId);

/// Spans for one body's type-reference arena.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct BodyTypeRefSourceMap(TypeRefSourceMap);

impl BodyTypeRefSourceMap {
    pub fn span(&self, id: BodyTypeRefId) -> text_size::TextRange {
        self.0.span(id.0)
    }
}

/// All type references written in one body, keyed by the nodes that carry
/// them. A missing key means the position had no written annotation.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct BodyTypeRefs {
    pub store: TypeRefStore,
    /// `Pattern::Type` nodes: `let x: T` ascriptions (as the bind's
    /// sub-pattern) and bare type patterns in match arms.
    pub pattern_types: FxHashMap<PatId, BodyTypeRefId>,
    /// Array-pattern `[...]: T` ascriptions.
    pub array_ascriptions: FxHashMap<PatId, BodyTypeRefId>,
    /// Class-destructure generic args (`Box<int> { v }` patterns), in
    /// written order. Never empty when present.
    pub pattern_class_args: FxHashMap<PatId, Box<[BodyTypeRefId]>>,
    /// Destructure-head associated bindings (`Source<Item = int> { v }`
    /// patterns), in written order. Never empty when present.
    pub pattern_assoc_bindings: FxHashMap<PatId, Box<[(Name, BodyTypeRefId)]>>,
    /// Explicit expression-position type arguments (`Call`, `GenericApply`,
    /// and `Object` constructors), in written order. Never empty when
    /// present.
    pub expr_type_args: FxHashMap<ExprId, Box<[BodyTypeRefId]>>,
    /// The STATIC right-hand sides of lexical `type T = ...` bindings. A
    /// binding whose right-hand side is `unreflect(expr)` has no entry: its
    /// operand is an expression in the body arena.
    pub stmt_type_bindings: FxHashMap<StmtId, BodyTypeRefId>,
    /// Written annotations on match scrutinees (`match (value: T)`).
    pub match_scrutinee_types: FxHashMap<ExprId, BodyTypeRefId>,
    /// `.as<T>` upcast targets.
    pub upcast_targets: FxHashMap<ExprId, BodyTypeRefId>,
    /// The two written halves of a `(Base as Interface).item` reference.
    pub qualified_path_anchors: FxHashMap<ExprId, QualifiedPathTypeRefs>,
    /// Lambda signature slots, keyed by the lambda expression.
    pub lambda_signatures: FxHashMap<ExprId, LambdaTypeRefs>,
}

/// The written halves of a fully-qualified item reference. Both are always
/// present — the parser builds the node only when it saw both — which is what
/// distinguishes this form from the two path spellings that leave one half to
/// inference.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct QualifiedPathTypeRefs {
    /// The `Self` type: `Base` in `(Base as Interface).item`.
    pub qself: BodyTypeRefId,
    /// The interface the item is projected through.
    pub interface: BodyTypeRefId,
}

/// A lambda's written signature slots (each optional: lambdas may omit any
/// of them and rely on expectation-driven inference).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LambdaTypeRefs {
    /// Per declared parameter, in order.
    pub params: Box<[Option<BodyTypeRefId>]>,
    pub return_type: Option<BodyTypeRefId>,
    pub throws: Option<BodyTypeRefId>,
}

impl BodyTypeRefs {
    pub fn raw_id(&self, id: BodyTypeRefId) -> TypeRefId {
        id.0
    }

    pub fn diagnostic_id(&self, id: TypeRefId) -> BodyTypeRefId {
        let _ = &self.store[id];
        BodyTypeRefId(id)
    }
}

/// Lowers every type expression in `body` into one store. Pure; ppir wraps
/// this in salsa queries over its canonical bodies.
pub fn collect_body_type_refs(body: &ExprBody) -> (BodyTypeRefs, BodyTypeRefSourceMap) {
    let mut builder = TypeRefBuilder::new();
    let mut refs = BodyTypeRefs::default();

    for (pat_id, pattern) in body.patterns.iter() {
        match pattern {
            Pattern::Type(type_expr) => {
                refs.pattern_types
                    .insert(pat_id, BodyTypeRefId(builder.lower(type_expr)));
            }
            Pattern::Array {
                ascription: Some(type_expr),
                ..
            } => {
                refs.array_ascriptions
                    .insert(pat_id, BodyTypeRefId(builder.lower(type_expr)));
            }
            Pattern::Class {
                generic_args,
                associated_type_bindings,
                ..
            } if !generic_args.is_empty() || !associated_type_bindings.is_empty() => {
                if !generic_args.is_empty() {
                    refs.pattern_class_args.insert(
                        pat_id,
                        generic_args
                            .iter()
                            .map(|arg| BodyTypeRefId(builder.lower(arg)))
                            .collect(),
                    );
                }
                if !associated_type_bindings.is_empty() {
                    refs.pattern_assoc_bindings.insert(
                        pat_id,
                        associated_type_bindings
                            .iter()
                            .map(|binding| {
                                (
                                    binding.name.clone(),
                                    BodyTypeRefId(builder.lower(&binding.ty)),
                                )
                            })
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
                    type_args
                        .iter()
                        .map(|arg| BodyTypeRefId(builder.lower(arg)))
                        .collect(),
                );
            }
            Expr::Upcast { target, .. } => {
                refs.upcast_targets
                    .insert(expr_id, BodyTypeRefId(builder.lower(target)));
            }
            Expr::QualifiedPath {
                qself, interface, ..
            } => {
                refs.qualified_path_anchors.insert(
                    expr_id,
                    QualifiedPathTypeRefs {
                        qself: BodyTypeRefId(builder.lower(qself)),
                        interface: BodyTypeRefId(builder.lower(interface)),
                    },
                );
            }
            Expr::Lambda(def) => {
                refs.lambda_signatures.insert(
                    expr_id,
                    LambdaTypeRefs {
                        params: def
                            .params
                            .iter()
                            .map(|param| {
                                param
                                    .type_expr
                                    .as_ref()
                                    .map(|ty| BodyTypeRefId(builder.lower(ty)))
                            })
                            .collect(),
                        return_type: def
                            .return_type
                            .as_ref()
                            .map(|ty| BodyTypeRefId(builder.lower(ty))),
                        throws: def
                            .throws
                            .as_ref()
                            .map(|ty| BodyTypeRefId(builder.lower(ty))),
                    },
                );
            }
            Expr::Match {
                scrutinee_type: Some(type_id),
                ..
            } => {
                refs.match_scrutinee_types.insert(
                    expr_id,
                    BodyTypeRefId(builder.lower(&body.type_annotations[*type_id])),
                );
            }
            _ => {}
        }
    }

    for (stmt_id, stmt) in body.stmts.iter() {
        if let Stmt::TypeBinding {
            value: TypeBindingValue::Static(value),
            ..
        } = stmt
        {
            refs.stmt_type_bindings
                .insert(stmt_id, BodyTypeRefId(builder.lower(value)));
        }
    }

    let (store, source_map) = builder.finish();
    refs.store = store;
    (refs, BodyTypeRefSourceMap(source_map))
}

#[cfg(test)]
mod tests {
    use baml_compiler2_ast::{Expr, ExprBody, Stmt, TypeExprKind};
    use text_size::{TextRange, TextSize};

    use super::*;
    use crate::type_ref::TypeRefKind;

    #[test]
    fn expression_type_arguments_preserve_order_and_identity() {
        let mut body = ExprBody::default();
        let callee = body.exprs.alloc(Expr::Path(vec![Name::new("f")]));
        let int_span = TextRange::new(TextSize::from(10), TextSize::from(13));
        let string_span = TextRange::new(TextSize::from(15), TextSize::from(21));
        let call = body.exprs.alloc(Expr::Call {
            callee,
            type_args: vec![
                TypeExprKind::Int { attrs: Vec::new() }.at(int_span),
                TypeExprKind::String { attrs: Vec::new() }.at(string_span),
            ],
            args: Vec::new(),
        });

        let (refs, source_map) = collect_body_type_refs(&body);
        let slots = refs.expr_type_args.get(&call).expect("call type slots");
        let [first, second] = slots.as_ref() else {
            panic!("expected two ordered slots, got {slots:?}");
        };
        assert_eq!(source_map.span(*first), int_span);
        assert_eq!(source_map.span(*second), string_span);
        assert!(matches!(
            refs.store.get(refs.raw_id(*second)).kind,
            TypeRefKind::String
        ));
    }

    #[test]
    fn only_static_type_binding_right_hand_sides_are_type_references() {
        let mut body = ExprBody::default();
        let operand = body
            .exprs
            .alloc(Expr::Path(vec![Name::new("runtime_type")]));
        let runtime = body.stmts.alloc(Stmt::TypeBinding {
            name: Name::new("R"),
            value: TypeBindingValue::Runtime(operand),
        });
        let static_span = TextRange::new(TextSize::from(30), TextSize::from(35));
        let static_binding = body.stmts.alloc(Stmt::TypeBinding {
            name: Name::new("S"),
            value: TypeBindingValue::Static(
                TypeExprKind::List {
                    inner: Box::new(TypeExprKind::Int { attrs: Vec::new() }.at(static_span)),
                    attrs: Vec::new(),
                }
                .at(static_span),
            ),
        });

        let (refs, source_map) = collect_body_type_refs(&body);
        assert!(
            !refs.stmt_type_bindings.contains_key(&runtime),
            "a runtime operand is a body expression, never a type reference"
        );
        let type_ref = refs
            .stmt_type_bindings
            .get(&static_binding)
            .copied()
            .expect("static binding type reference");
        assert!(matches!(
            refs.store.get(refs.raw_id(type_ref)).kind,
            TypeRefKind::List { .. }
        ));
        assert_eq!(source_map.span(type_ref), static_span);
    }
}
