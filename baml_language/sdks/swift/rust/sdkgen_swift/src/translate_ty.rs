//! BAML `Ty` → Swift type expression translation.
//!
//! Phase 2 supports primitives, lists/maps/optionals, and references
//! to *supported* user classes, enums, and (non-recursive) type
//! aliases. [`translate_ty`] returns `None` for anything not yet
//! supported, and the emitter skips whole symbols whose signature
//! contains an unsupported type — the generated package must always
//! compile, and capabilities widen phase by phase.

use std::collections::BTreeSet;

use baml_codegen_types::{Name, Ty};

/// Which named types the emitter decided it can emit this run (the
/// fixpoint result from `lib.rs`), plus how to spell their Swift path.
pub(crate) struct TranslateCtx {
    /// FQNs of user classes that will be emitted.
    pub supported_classes: BTreeSet<String>,
    /// FQNs of user enums that will be emitted.
    pub supported_enums: BTreeSet<String>,
    /// FQNs of user type aliases that will be emitted.
    pub supported_aliases: BTreeSet<String>,
}

impl TranslateCtx {
    fn named_ref(&self, name: &Name, pool: &BTreeSet<String>) -> Option<String> {
        if name.pkg.as_str() != "user" || name.is_stream() {
            return None;
        }
        let fqn = name.to_string();
        if pool.contains(&fqn) {
            Some(swift_type_path(name))
        } else {
            None
        }
    }
}

/// Fully-qualified Swift spelling of a generated type: the `Baml`
/// namespace-enum tree mirrors the BAML namespace path. Always
/// qualified so cross-namespace references need no imports.
pub(crate) fn swift_type_path(name: &Name) -> String {
    let mut out = String::from("Baml");
    for seg in &name.namespace_path {
        out.push('.');
        out.push_str(&crate::escape_ident(seg.as_str()));
    }
    out.push('.');
    out.push_str(&crate::escape_ident(name.bare_name()));
    out
}

/// Swift spelling of `ty`, or `None` if the type is not yet supported.
pub(crate) fn translate_ty(ty: &Ty, ctx: &TranslateCtx) -> Option<String> {
    match ty {
        Ty::Int => Some("Int".to_string()),
        // BAML float is f64.
        Ty::Float => Some("Double".to_string()),
        Ty::String => Some("String".to_string()),
        Ty::Bool => Some("Bool".to_string()),
        // Standalone `null` type: Swift has no untyped nil, so it gets
        // a unit-like runtime type that encodes/decodes as BAML null.
        Ty::Null => Some("BamlNull".to_string()),
        Ty::Uint8Array => Some("Data".to_string()),
        // Literal types collapse to their base type; Swift has no
        // literal types and the engine re-validates values anyway.
        Ty::Literal(lit) => Some(
            match lit {
                baml_base::Literal::String(_) => "String",
                baml_base::Literal::Int(_) => "Int",
                baml_base::Literal::Bool(_) => "Bool",
                // Bigint / float literals: unsupported for now.
                _ => return None,
            }
            .to_string(),
        ),
        Ty::List(inner) => Some(format!("[{}]", translate_ty(inner, ctx)?)),
        Ty::Map { key, value } => {
            // BAML map keys are stringified engine-side; only string
            // keys are supported host-side for now (mirrors Python's
            // dict[str, Any] posture on the decode path).
            if !matches!(**key, Ty::String) {
                return None;
            }
            Some(format!("[String: {}]", translate_ty(value, ctx)?))
        }
        Ty::Union(members) => translate_null_union(members, ctx),
        // Generic classes are Phase 5; non-generic references resolve
        // against the fixpoint's supported set.
        Ty::Class(name, args) => {
            if !args.is_empty() {
                return None;
            }
            ctx.named_ref(name, &ctx.supported_classes)
        }
        Ty::Enum(name) => ctx.named_ref(name, &ctx.supported_enums),
        Ty::TypeAlias(name) => ctx.named_ref(name, &ctx.supported_aliases),
        // Unit is only meaningful in return position; the emitter
        // special-cases it. Everything else lands in later phases.
        _ => None,
    }
}

/// Collapse a null-bearing union with exactly one other supported
/// member into `T?`. Any other union shape is unsupported until the
/// generated-union-enum phase.
fn translate_null_union(members: &[Ty], ctx: &TranslateCtx) -> Option<String> {
    let (nulls, non_nulls): (Vec<&Ty>, Vec<&Ty>) =
        members.iter().partition(|t| matches!(t, Ty::Null));
    if nulls.is_empty() || non_nulls.len() != 1 {
        return None;
    }
    Some(format!("{}?", translate_ty(non_nulls[0], ctx)?))
}

/// For an optional-argument slot: the `T` in `BamlOptional<T>`. A
/// nullable declared type contributes its non-null part (the `.null`
/// case covers the rest); a non-nullable defaulted type is used as-is.
pub(crate) fn translate_optional_arg_inner(ty: &Ty, ctx: &TranslateCtx) -> Option<String> {
    if let Ty::Union(members) = ty {
        let non_nulls: Vec<&Ty> = members.iter().filter(|t| !matches!(t, Ty::Null)).collect();
        if members.len() != non_nulls.len() && non_nulls.len() == 1 {
            return translate_ty(non_nulls[0], ctx);
        }
    }
    translate_ty(ty, ctx)
}
