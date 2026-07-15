//! BAML `Ty` → Swift type expression translation.
//!
//! Phase 1 supports the primitive subset (plus lists/maps/optionals of
//! it). [`translate_ty`] returns `None` for anything not yet
//! supported, and the emitter skips whole functions whose signature
//! contains an unsupported type — the generated package must always
//! compile, and capabilities are turned on by widening this function.

use baml_codegen_types::Ty;

/// Swift spelling of `ty`, or `None` if the type is not yet supported.
pub(crate) fn translate_ty(ty: &Ty) -> Option<String> {
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
        Ty::List(inner) => Some(format!("[{}]", translate_ty(inner)?)),
        Ty::Map { key, value } => {
            // BAML map keys are stringified engine-side; only string
            // keys are supported host-side for now (mirrors Python's
            // dict[str, Any] posture on the decode path).
            if !matches!(**key, Ty::String) {
                return None;
            }
            Some(format!("[String: {}]", translate_ty(value)?))
        }
        Ty::Union(members) => translate_null_union(members),
        // Unit is only meaningful in return position; the emitter
        // special-cases it. Everything else lands in later phases.
        _ => None,
    }
}

/// Collapse a null-bearing union with exactly one other supported
/// member into `T?`. Any other union shape is unsupported until the
/// generated-union-enum phase.
fn translate_null_union(members: &[Ty]) -> Option<String> {
    let (nulls, non_nulls): (Vec<&Ty>, Vec<&Ty>) =
        members.iter().partition(|t| matches!(t, Ty::Null));
    if nulls.is_empty() || non_nulls.len() != 1 {
        return None;
    }
    Some(format!("{}?", translate_ty(non_nulls[0])?))
}
