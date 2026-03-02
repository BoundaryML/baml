//! Union simplification and stream field type computation.
//!
//! Phase 3 core: given a field's normalized annotations (D, S, typeof(S)),
//! compute the simplified stream field type as `simplify(typeof(S) | D)`.

use crate::normalize::NormalizedStreamField;
use crate::ty::{PpirTy, PpirTypeAttrs};

/// Simplify a union of `PpirTy` variants.
///
/// Applies the simplification rules from the stream-types spec:
/// - Flatten nested unions
/// - Remove `never` variants
/// - Subsume literals when parent primitive is present
///   (`"foo" | string → string`, `42 | int → int`, etc.)
/// - Dedup identical variants
/// - Collapse: 0 variants → never, 1 variant → unwrap, 2+ → union
pub fn simplify_union(variants: Vec<PpirTy>) -> PpirTy {
    // 1. Flatten nested unions
    let mut flat: Vec<PpirTy> = Vec::new();
    for v in variants {
        match v {
            PpirTy::Union { variants: inner, .. } => flat.extend(inner),
            other => flat.push(other),
        }
    }

    // 2. Remove `never` variants
    flat.retain(|v| !matches!(v, PpirTy::Never { .. }));

    // 3. Subsumption: remove literals when parent primitive is present
    let has_string = flat.iter().any(|v| matches!(v, PpirTy::String { .. }));
    let has_int = flat.iter().any(|v| matches!(v, PpirTy::Int { .. }));
    let _has_float = flat.iter().any(|v| matches!(v, PpirTy::Float { .. }));
    let has_bool = flat.iter().any(|v| matches!(v, PpirTy::Bool { .. }));

    if has_string {
        flat.retain(|v| !matches!(v, PpirTy::StringLiteral { .. }));
    }
    if has_int {
        flat.retain(|v| !matches!(v, PpirTy::IntLiteral { .. }));
    }
    // Float literals in PPIR are represented as PpirTy::Float
    // (see infer_typeof_s), so float subsumption is a no-op in practice.
    if has_bool {
        flat.retain(|v| !matches!(v, PpirTy::BoolLiteral { .. }));
    }

    // 4. Structural subsumption: container bottom types
    //    list<never> subsumed by list<T>, map<K, never> subsumed by map<K, V>
    if flat.len() >= 2 {
        let has_real_list = flat.iter().any(|v| matches!(v, PpirTy::List { inner, .. } if !matches!(**inner, PpirTy::Never { .. })));
        let has_real_map = flat.iter().any(|v| matches!(v, PpirTy::Map { value, .. } if !matches!(**value, PpirTy::Never { .. })));
        if has_real_list {
            flat.retain(|v| !matches!(v, PpirTy::List { inner, .. } if matches!(**inner, PpirTy::Never { .. })));
        }
        if has_real_map {
            flat.retain(|v| !matches!(v, PpirTy::Map { value, .. } if matches!(**value, PpirTy::Never { .. })));
        }
    }

    // 5. Dedup (preserving order)
    let mut seen: Vec<PpirTy> = Vec::new();
    flat.retain(|v| {
        if seen.contains(v) {
            false
        } else {
            seen.push(v.clone());
            true
        }
    });

    // 6. Collapse
    let d = PpirTypeAttrs::default();
    match flat.len() {
        0 => PpirTy::Never { attrs: d },
        1 => flat.into_iter().next().unwrap(),
        _ => PpirTy::Union { variants: flat, attrs: d },
    }
}

/// Compute `simplify(typeof(S) | D)` for a single field.
///
/// This is the core of Phase 3's per-field type computation.
///
/// - If `typeof_s` is `Some(t)`, uses `simplify(t | D)`
/// - If `typeof_s` is `None` (EmptyList/EmptyMap), uses `simplify(never | D)` = `D`
///   because the empty container is always subsumed by D's container type.
pub fn compute_stream_field_type(normalized: &NormalizedStreamField) -> PpirTy {
    let typeof_s = match &normalized.typeof_s {
        Some(t) => t.clone(),
        // EmptyList/EmptyMap: typeof([]) and typeof({}) are deferred.
        // Since [] | T[] → T[] and {} | map<K,V> → map<K,V>,
        // the empty container is always subsumed. Use Never so
        // simplify(never | D) = D.
        None => PpirTy::Never { attrs: PpirTypeAttrs::default() },
    };

    simplify_union(vec![typeof_s, normalized.stream_type.clone()])
}

#[cfg(test)]
mod tests {
    use super::*;

    fn d() -> PpirTypeAttrs {
        PpirTypeAttrs::default()
    }

    #[test]
    fn simplify_removes_never() {
        let result = simplify_union(vec![
            PpirTy::Never { attrs: d() },
            PpirTy::String { attrs: d() },
        ]);
        assert_eq!(result, PpirTy::String { attrs: d() });
    }

    #[test]
    fn simplify_never_only_is_never() {
        let result = simplify_union(vec![PpirTy::Never { attrs: d() }]);
        assert_eq!(result, PpirTy::Never { attrs: d() });
    }

    #[test]
    fn simplify_empty_is_never() {
        let result = simplify_union(vec![]);
        assert_eq!(result, PpirTy::Never { attrs: d() });
    }

    #[test]
    fn simplify_null_and_string() {
        let result = simplify_union(vec![
            PpirTy::Null { attrs: d() },
            PpirTy::String { attrs: d() },
        ]);
        assert_eq!(
            result,
            PpirTy::Union {
                variants: vec![PpirTy::Null { attrs: d() }, PpirTy::String { attrs: d() }],
                attrs: d(),
            }
        );
    }

    #[test]
    fn simplify_subsumes_string_literal() {
        let result = simplify_union(vec![
            PpirTy::StringLiteral { value: "foo".into(), attrs: d() },
            PpirTy::String { attrs: d() },
        ]);
        assert_eq!(result, PpirTy::String { attrs: d() });
    }

    #[test]
    fn simplify_subsumes_int_literal() {
        let result = simplify_union(vec![
            PpirTy::IntLiteral { value: 42, attrs: d() },
            PpirTy::Int { attrs: d() },
        ]);
        assert_eq!(result, PpirTy::Int { attrs: d() });
    }

    #[test]
    fn simplify_subsumes_bool_literal() {
        let result = simplify_union(vec![
            PpirTy::BoolLiteral { value: true, attrs: d() },
            PpirTy::Bool { attrs: d() },
        ]);
        assert_eq!(result, PpirTy::Bool { attrs: d() });
    }

    #[test]
    fn simplify_dedup() {
        let result = simplify_union(vec![
            PpirTy::String { attrs: d() },
            PpirTy::String { attrs: d() },
        ]);
        assert_eq!(result, PpirTy::String { attrs: d() });
    }

    #[test]
    fn simplify_flattens_nested_unions() {
        let inner = PpirTy::Union {
            variants: vec![PpirTy::Int { attrs: d() }, PpirTy::String { attrs: d() }],
            attrs: d(),
        };
        let result = simplify_union(vec![inner, PpirTy::Bool { attrs: d() }]);
        assert_eq!(
            result,
            PpirTy::Union {
                variants: vec![
                    PpirTy::Int { attrs: d() },
                    PpirTy::String { attrs: d() },
                    PpirTy::Bool { attrs: d() },
                ],
                attrs: d(),
            }
        );
    }

    #[test]
    fn compute_field_type_null_and_string() {
        // S=null, D=string → simplify(null | string) = null | string
        let norm = NormalizedStreamField {
            name: "test".into(),
            stream_type: PpirTy::String { attrs: d() },
            in_progress_never: false,
            starts_as: crate::normalize::StartsAs::Null,
            typeof_s: Some(PpirTy::Null { attrs: d() }),
        };
        let result = compute_stream_field_type(&norm);
        assert_eq!(
            result,
            PpirTy::Union {
                variants: vec![PpirTy::Null { attrs: d() }, PpirTy::String { attrs: d() }],
                attrs: d(),
            }
        );
    }

    #[test]
    fn compute_field_type_empty_list_subsumed() {
        // S=[], D=int[] → typeof_s=None → simplify(never | int[]) = int[]
        let norm = NormalizedStreamField {
            name: "test".into(),
            stream_type: PpirTy::List {
                inner: Box::new(PpirTy::Int { attrs: d() }),
                attrs: d(),
            },
            in_progress_never: false,
            starts_as: crate::normalize::StartsAs::EmptyList,
            typeof_s: None,
        };
        let result = compute_stream_field_type(&norm);
        assert_eq!(
            result,
            PpirTy::List {
                inner: Box::new(PpirTy::Int { attrs: d() }),
                attrs: d(),
            }
        );
    }

    #[test]
    fn compute_field_type_literal_subsumed() {
        // S="Loading...", D=string → simplify("Loading..." | string) = string
        let norm = NormalizedStreamField {
            name: "test".into(),
            stream_type: PpirTy::String { attrs: d() },
            in_progress_never: false,
            starts_as: crate::normalize::StartsAs::Literal(
                crate::normalize::StartsAsLiteral::String("Loading...".into()),
            ),
            typeof_s: Some(PpirTy::StringLiteral {
                value: "Loading...".into(),
                attrs: d(),
            }),
        };
        let result = compute_stream_field_type(&norm);
        assert_eq!(result, PpirTy::String { attrs: d() });
    }

    #[test]
    fn compute_field_type_never_d_string() {
        // S=never, D=string → simplify(never | string) = string
        let norm = NormalizedStreamField {
            name: "test".into(),
            stream_type: PpirTy::String { attrs: d() },
            in_progress_never: false,
            starts_as: crate::normalize::StartsAs::Never,
            typeof_s: Some(PpirTy::Never { attrs: d() }),
        };
        let result = compute_stream_field_type(&norm);
        assert_eq!(result, PpirTy::String { attrs: d() });
    }
}
