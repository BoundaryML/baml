//! Shared test-only fixture DSL for `Ty` construction and a reusable
//! `PatCtx` implementation (`TestingCtx`).
//!
//! This module is `#[cfg(test)]`-only. It consolidates the `Ty`-builder
//! helpers and the `PatCtx` test fixture previously duplicated across the
//! `exhaustiveness` and `normalize` test modules. Values produced here are
//! bit-identical to the originals (same `TyAttr::default()`, same
//! `Freshness::Regular`), so no assertion outcomes change.

#![allow(
    clippy::default_trait_access,
    clippy::doc_markdown,
    clippy::items_after_statements,
    clippy::cloned_ref_to_slice_refs,
    clippy::many_single_char_names,
    clippy::redundant_clone,
    clippy::redundant_closure_for_method_calls,
    clippy::single_char_pattern,
    clippy::uninlined_format_args,
    clippy::unnested_or_patterns,
    dead_code
)]

use baml_base::{Literal, Name};

use crate::{
    exhaustiveness::*,
    ty::{Freshness, FunctionParamTy, PrimitiveType, QualifiedTypeName, Ty, TyAttr},
};

// ── Ty-builder DSL (shared) ──────────────────────────────────────────────────

pub(crate) fn bool_lit(v: bool) -> Ty {
    Ty::Literal(Literal::Bool(v), Freshness::Regular, Default::default())
}
pub(crate) fn int_lit(v: i64) -> Ty {
    Ty::Literal(Literal::Int(v), Freshness::Regular, Default::default())
}
pub(crate) fn float_lit(s: &str) -> Ty {
    Ty::Literal(
        Literal::Float(s.into()),
        Freshness::Regular,
        Default::default(),
    )
}
pub(crate) fn bool_ty() -> Ty {
    Ty::Primitive(PrimitiveType::Bool, Default::default())
}
pub(crate) fn int_ty() -> Ty {
    Ty::Primitive(PrimitiveType::Int, Default::default())
}

/// Stringify a report's missing-case witnesses for assertions.
pub(crate) fn missing_strings(report: &UsefulnessReport) -> Vec<String> {
    report.missing.iter().map(ToString::to_string).collect()
}

pub(crate) fn class_ty(q: &QualifiedTypeName) -> Ty {
    Ty::Class(q.clone(), vec![], Default::default())
}
pub(crate) fn list_of(elem: Ty) -> Ty {
    Ty::List(Box::new(elem), Default::default())
}
pub(crate) fn opt_of(t: Ty) -> Ty {
    Ty::Optional(Box::new(t), Default::default())
}
pub(crate) fn union_of(ts: Vec<Ty>) -> Ty {
    Ty::Union(ts, Default::default())
}
pub(crate) fn null_ty() -> Ty {
    Ty::Primitive(PrimitiveType::Null, Default::default())
}
pub(crate) fn never_ty() -> Ty {
    Ty::Never {
        attr: Default::default(),
    }
}

/// A `QualifiedTypeName` in the implicit `user` package (exhaustiveness DSL).
pub(crate) fn qtn(name: &str) -> QualifiedTypeName {
    QualifiedTypeName::new(Name::new("user"), vec![], Name::new(name))
}

// ── normalize-specific helpers ───────────────────────────────────────────────
//
// `qn` lives in the `test` package and is intentionally DISTINCT from `qtn`
// (package `user`): they differ in package and that difference is load-bearing
// for QTN-identity tests.

/// A `QualifiedTypeName` in the `test` package (normalize DSL).
pub(crate) fn qn(name: &str) -> QualifiedTypeName {
    QualifiedTypeName::new(Name::new("test"), vec![], Name::new(name))
}

pub(crate) fn type_alias(name: &str) -> Ty {
    Ty::TypeAlias(qn(name), TyAttr::default())
}

pub(crate) fn required_param(ty: Ty) -> FunctionParamTy {
    FunctionParamTy::required(None, ty)
}

pub(crate) fn optional_param(name: &str, ty: Ty) -> FunctionParamTy {
    FunctionParamTy::optional(Some(Name::new(name)), ty)
}

// Simple Ty-variant builders normalize uses heavily.
pub(crate) fn string_ty() -> Ty {
    Ty::Primitive(PrimitiveType::String, Default::default())
}
pub(crate) fn float_ty() -> Ty {
    Ty::Primitive(PrimitiveType::Float, Default::default())
}
pub(crate) fn bigint_ty() -> Ty {
    Ty::Primitive(PrimitiveType::Bigint, Default::default())
}

// ── Reusable PatCtx test fixture ─────────────────────────────────────────────
//
// A reusable test ctx that supports a class registry. Classes are registered
// by name with their field types in declaration order. `Optional<T>` is
// enumerated as inner-ctors + null; `List<T>` returns NonExhaustive in
// `enumerate_ctors` because slice splitting is handled in `split_ctors`
// (which special-cases List).

pub(crate) struct TestingCtx {
    classes: std::collections::HashMap<QualifiedTypeName, Vec<Ty>>,
    /// Type alias map: `Ty::TypeAlias(qtn)` resolves to the target Ty.
    /// Mirrors the real builder's `expand_alias_chains` behaviour.
    aliases: std::collections::HashMap<QualifiedTypeName, Ty>,
    /// When set, `Ty::Union` enumerates each member as a `UnionMember`
    /// ctor (discriminating branches) instead of flattening member ctors.
    /// Mirrors what the real builder does for union scrutinees.
    union_as_members: bool,
}
impl TestingCtx {
    pub(crate) fn new() -> Self {
        Self {
            classes: std::collections::HashMap::new(),
            aliases: std::collections::HashMap::new(),
            union_as_members: false,
        }
    }
    pub(crate) fn with_union_members(mut self) -> Self {
        self.union_as_members = true;
        self
    }
    pub(crate) fn register(&mut self, qtn: QualifiedTypeName, fields: Vec<Ty>) {
        self.classes.insert(qtn, fields);
    }
    pub(crate) fn register_alias(&mut self, qtn: QualifiedTypeName, target: Ty) {
        self.aliases.insert(qtn, target);
    }
    /// Walk through `Ty::TypeAlias` chains to a non-alias type. Cycles
    /// fall through to the original (caller treats as opaque).
    fn expand_alias(&self, ty: &Ty) -> Ty {
        let mut current = ty.clone();
        let mut seen: std::collections::HashSet<QualifiedTypeName> =
            std::collections::HashSet::new();
        while let Ty::TypeAlias(qtn, _) = &current {
            if !seen.insert(qtn.clone()) {
                return current;
            }
            match self.aliases.get(qtn) {
                Some(target) => current = target.clone(),
                None => return current,
            }
        }
        current
    }
}
impl PatCtx for TestingCtx {
    fn enumerate_ctors(&self, ty: &Ty) -> Vec<Ctor> {
        // Peel aliases first, the same way the real builder will.
        let ty = self.expand_alias(ty);
        match &ty {
            Ty::Primitive(PrimitiveType::Bool, _) => {
                vec![Ctor::Single(bool_lit(true)), Ctor::Single(bool_lit(false))]
            }
            Ty::Primitive(PrimitiveType::Int, _)
            | Ty::Primitive(PrimitiveType::Float, _)
            | Ty::Primitive(PrimitiveType::String, _) => vec![Ctor::NonExhaustive],
            Ty::Primitive(PrimitiveType::Null, _) => vec![Ctor::Single(ty.clone())],
            Ty::Optional(inner, _) => {
                let mut out = self.enumerate_ctors(inner);
                out.push(Ctor::Single(Ty::Primitive(
                    PrimitiveType::Null,
                    Default::default(),
                )));
                out
            }
            Ty::Union(members, _) if self.union_as_members => members
                .iter()
                .map(|m| Ctor::UnionMember(m.clone()))
                .collect(),
            Ty::Union(members, _) => members
                .iter()
                .flat_map(|m| self.enumerate_ctors(m))
                .collect(),
            Ty::Literal(_, _, _) | Ty::EnumVariant(_, _, _) => {
                vec![Ctor::Single(ty.clone())]
            }
            Ty::Class(qtn, args, _) => vec![Ctor::Class(qtn.clone(), args.clone())],
            // For slices, split_ctors handles enumeration via slice splitting;
            // returning NonExhaustive here is OK because the slice path is taken
            // before this is consulted.
            Ty::List(_, _) | Ty::EvolvingList(_, _) => vec![Ctor::NonExhaustive],
            Ty::Never { .. } => vec![],
            Ty::TypeVar(_, _) => vec![Ctor::NonExhaustive],
            _ => vec![Ctor::NonExhaustive],
        }
    }
    fn class_field_types(&self, qtn: &QualifiedTypeName, _ty: &Ty) -> Vec<Ty> {
        self.classes.get(qtn).cloned().unwrap_or_default()
    }
    fn list_element_type(&self, ty: &Ty) -> Ty {
        match self.expand_alias(ty) {
            Ty::List(e, _) | Ty::EvolvingList(e, _) => (*e).clone(),
            t => t,
        }
    }
}
