//! Unit tests for PPIR stream expansion and normalization.

use smol_str::SmolStr;

use crate::{
    expand::{
        default_sap_missing, default_starts_as, make_union, stream_expand, PpirExpandedField,
        PpirSapMissing,
    },
    normalize::{
        StartsAs, StartsAsLiteral, default_starts_as_semantic, infer_typeof_s,
        parse_starts_as_value,
    },
    ty::{PpirTy, PpirTypeAttrs},
};

// ─────────────────────────────── helpers ──────────────────────────────────────

/// Shorthand for default attrs.
fn d() -> PpirTypeAttrs {
    PpirTypeAttrs::default()
}

// ─────────────────────────────── make_union tests ────────────────────────────

#[test]
fn make_union_both_never() {
    let result = make_union(
        PpirTy::Never { attrs: d() },
        PpirTy::Never { attrs: d() },
    );
    assert_eq!(result, PpirTy::Never { attrs: d() });
}

#[test]
fn make_union_s_never() {
    let result = make_union(PpirTy::Never { attrs: d() }, PpirTy::Int { attrs: d() });
    assert_eq!(result, PpirTy::Int { attrs: d() });
}

#[test]
fn make_union_d_never() {
    let result = make_union(PpirTy::Int { attrs: d() }, PpirTy::Never { attrs: d() });
    assert_eq!(result, PpirTy::Int { attrs: d() });
}

#[test]
fn make_union_same() {
    let result = make_union(PpirTy::Null { attrs: d() }, PpirTy::Null { attrs: d() });
    assert_eq!(result, PpirTy::Null { attrs: d() });
}

#[test]
fn make_union_different() {
    let result = make_union(PpirTy::Null { attrs: d() }, PpirTy::Int { attrs: d() });
    assert_eq!(
        result,
        PpirTy::union(vec![PpirTy::Null { attrs: d() }, PpirTy::Int { attrs: d() }])
    );
}

// ─────────────────────────────── default_starts_as tests ─────────────────────

#[test]
fn default_starts_as_primitive() {
    assert_eq!(default_starts_as(&PpirTy::Int { attrs: d() }), PpirTy::Null { attrs: d() });
    assert_eq!(default_starts_as(&PpirTy::String { attrs: d() }), PpirTy::Null { attrs: d() });
    assert_eq!(default_starts_as(&PpirTy::Bool { attrs: d() }), PpirTy::Null { attrs: d() });
    assert_eq!(default_starts_as(&PpirTy::Float { attrs: d() }), PpirTy::Null { attrs: d() });
}

#[test]
fn default_starts_as_literal() {
    assert_eq!(
        default_starts_as(&PpirTy::StringLiteral { value: "foo".to_string(), attrs: d() }),
        PpirTy::Never { attrs: d() }
    );
    assert_eq!(
        default_starts_as(&PpirTy::IntLiteral { value: 42, attrs: d() }),
        PpirTy::Never { attrs: d() }
    );
    assert_eq!(
        default_starts_as(&PpirTy::BoolLiteral { value: true, attrs: d() }),
        PpirTy::Never { attrs: d() }
    );
}

#[test]
fn default_starts_as_never() {
    assert_eq!(
        default_starts_as(&PpirTy::Never { attrs: d() }),
        PpirTy::Never { attrs: d() }
    );
}

#[test]
fn default_starts_as_null() {
    assert_eq!(
        default_starts_as(&PpirTy::Null { attrs: d() }),
        PpirTy::Null { attrs: d() }
    );
}

#[test]
fn default_starts_as_list() {
    let ty = PpirTy::list(PpirTy::Int { attrs: d() });
    let s = default_starts_as(&ty);
    assert_eq!(s, PpirTy::list(PpirTy::Never { attrs: d() }));
}

#[test]
fn default_starts_as_map() {
    let ty = PpirTy::Map {
        key: Box::new(PpirTy::String { attrs: d() }),
        value: Box::new(PpirTy::Int { attrs: d() }),
        attrs: d(),
    };
    let s = default_starts_as(&ty);
    assert_eq!(
        s,
        PpirTy::Map {
            key: Box::new(PpirTy::String { attrs: d() }),
            value: Box::new(PpirTy::Never { attrs: d() }),
            attrs: d(),
        }
    );
}

#[test]
fn default_starts_as_named_type() {
    let ty = PpirTy::named(SmolStr::new("stream_Resume"));
    assert_eq!(default_starts_as(&ty), PpirTy::Null { attrs: d() });
}

// ─────────────────────────────── default_sap_missing tests ───────────────────

#[test]
fn default_sap_missing_primitive() {
    match default_sap_missing(&PpirTy::Int { attrs: d() }) {
        PpirSapMissing::Default(ty) => assert_eq!(ty, PpirTy::Null { attrs: d() }),
        other => panic!("expected Default, got {other:?}"),
    }
}

#[test]
fn default_sap_missing_literal() {
    assert_eq!(
        default_sap_missing(&PpirTy::StringLiteral { value: "foo".to_string(), attrs: d() }),
        PpirSapMissing::Never
    );
}

#[test]
fn default_sap_missing_never() {
    assert_eq!(
        default_sap_missing(&PpirTy::Never { attrs: d() }),
        PpirSapMissing::Never
    );
}

#[test]
fn default_sap_missing_list() {
    match default_sap_missing(&PpirTy::list(PpirTy::Int { attrs: d() })) {
        PpirSapMissing::Default(ty) => {
            assert_eq!(ty, PpirTy::list(PpirTy::Never { attrs: d() }));
        }
        other => panic!("expected Default, got {other:?}"),
    }
}

// ─────────────────────────────── stream_expand tests ─────────────────────────
//
// Note: stream_expand requires PpirNames + db, which requires a Salsa runtime.
// These tests are kept as documentation; integration tests exercise stream_expand
// via the full pipeline.
//
// For unit testing stream_expand behavior, we test the helper functions
// (default_starts_as, default_sap_missing, make_union) instead.

// ─────────────────────────────── parse_starts_as_value tests ─────────────────

#[test]
fn parse_starts_as_never() {
    assert_eq!(parse_starts_as_value("never"), StartsAs::Never);
}

#[test]
fn parse_starts_as_null() {
    assert_eq!(parse_starts_as_value("null"), StartsAs::Null);
}

#[test]
fn parse_starts_as_true() {
    assert_eq!(
        parse_starts_as_value("true"),
        StartsAs::Literal(StartsAsLiteral::Bool(true))
    );
}

#[test]
fn parse_starts_as_false() {
    assert_eq!(
        parse_starts_as_value("false"),
        StartsAs::Literal(StartsAsLiteral::Bool(false))
    );
}

#[test]
fn parse_starts_as_int() {
    assert_eq!(
        parse_starts_as_value("42"),
        StartsAs::Literal(StartsAsLiteral::Int(42))
    );
}

#[test]
fn parse_starts_as_negative_int() {
    assert_eq!(
        parse_starts_as_value("-1"),
        StartsAs::Literal(StartsAsLiteral::Int(-1))
    );
}

#[test]
fn parse_starts_as_float() {
    assert_eq!(
        parse_starts_as_value("3.14"),
        StartsAs::Literal(StartsAsLiteral::Float("3.14".to_string()))
    );
}

#[test]
fn parse_starts_as_empty_list() {
    assert_eq!(parse_starts_as_value("[]"), StartsAs::EmptyList);
}

#[test]
fn parse_starts_as_empty_map() {
    assert_eq!(parse_starts_as_value("{}"), StartsAs::EmptyMap);
}

#[test]
fn parse_starts_as_string() {
    assert_eq!(
        parse_starts_as_value("Loading..."),
        StartsAs::Literal(StartsAsLiteral::String("Loading...".to_string()))
    );
}

// ─────────────────────────────── default_starts_as_semantic tests ─────────────

#[test]
fn default_starts_as_semantic_primitive() {
    assert_eq!(default_starts_as_semantic(&PpirTy::Int { attrs: d() }), StartsAs::Null);
    assert_eq!(default_starts_as_semantic(&PpirTy::String { attrs: d() }), StartsAs::Null);
    assert_eq!(default_starts_as_semantic(&PpirTy::Bool { attrs: d() }), StartsAs::Null);
    assert_eq!(default_starts_as_semantic(&PpirTy::Float { attrs: d() }), StartsAs::Null);
}

#[test]
fn default_starts_as_semantic_literal() {
    assert_eq!(
        default_starts_as_semantic(&PpirTy::StringLiteral { value: "foo".to_string(), attrs: d() }),
        StartsAs::Never
    );
    assert_eq!(
        default_starts_as_semantic(&PpirTy::IntLiteral { value: 42, attrs: d() }),
        StartsAs::Never
    );
    assert_eq!(
        default_starts_as_semantic(&PpirTy::BoolLiteral { value: true, attrs: d() }),
        StartsAs::Never
    );
}

#[test]
fn default_starts_as_semantic_list() {
    assert_eq!(
        default_starts_as_semantic(&PpirTy::list(PpirTy::Int { attrs: d() })),
        StartsAs::EmptyList
    );
}

#[test]
fn default_starts_as_semantic_map() {
    let ty = PpirTy::Map {
        key: Box::new(PpirTy::String { attrs: d() }),
        value: Box::new(PpirTy::Int { attrs: d() }),
        attrs: d(),
    };
    assert_eq!(default_starts_as_semantic(&ty), StartsAs::EmptyMap);
}

#[test]
fn default_starts_as_semantic_named() {
    assert_eq!(
        default_starts_as_semantic(&PpirTy::named("stream_Resume")),
        StartsAs::Null
    );
}

#[test]
fn default_starts_as_semantic_never() {
    assert_eq!(
        default_starts_as_semantic(&PpirTy::Never { attrs: d() }),
        StartsAs::Never
    );
}

#[test]
fn default_starts_as_semantic_null() {
    assert_eq!(
        default_starts_as_semantic(&PpirTy::Null { attrs: d() }),
        StartsAs::Null
    );
}

// ─────────────────────────────── infer_typeof_s tests ─────────────────────────

#[test]
fn infer_typeof_s_never() {
    assert_eq!(infer_typeof_s(&StartsAs::Never), Some(PpirTy::Never { attrs: d() }));
}

#[test]
fn infer_typeof_s_null() {
    assert_eq!(infer_typeof_s(&StartsAs::Null), Some(PpirTy::Null { attrs: d() }));
}

#[test]
fn infer_typeof_s_string_literal() {
    assert_eq!(
        infer_typeof_s(&StartsAs::Literal(StartsAsLiteral::String(
            "Loading...".to_string()
        ))),
        Some(PpirTy::StringLiteral { value: "Loading...".to_string(), attrs: d() })
    );
}

#[test]
fn infer_typeof_s_int_literal() {
    assert_eq!(
        infer_typeof_s(&StartsAs::Literal(StartsAsLiteral::Int(0))),
        Some(PpirTy::IntLiteral { value: 0, attrs: d() })
    );
}

#[test]
fn infer_typeof_s_bool_literal() {
    assert_eq!(
        infer_typeof_s(&StartsAs::Literal(StartsAsLiteral::Bool(false))),
        Some(PpirTy::BoolLiteral { value: false, attrs: d() })
    );
}

#[test]
fn infer_typeof_s_float_literal() {
    assert_eq!(
        infer_typeof_s(&StartsAs::Literal(StartsAsLiteral::Float(
            "3.14".to_string()
        ))),
        Some(PpirTy::Float { attrs: d() })
    );
}

#[test]
fn infer_typeof_s_empty_list() {
    assert_eq!(infer_typeof_s(&StartsAs::EmptyList), None);
}

#[test]
fn infer_typeof_s_empty_map() {
    assert_eq!(infer_typeof_s(&StartsAs::EmptyMap), None);
}

// ─────────────────────────────── PpirTy constructor tests ────────────────────

#[test]
fn ppir_ty_from_type_name_primitives() {
    assert_eq!(PpirTy::from_type_name("int"), PpirTy::Int { attrs: d() });
    assert_eq!(PpirTy::from_type_name("float"), PpirTy::Float { attrs: d() });
    assert_eq!(PpirTy::from_type_name("string"), PpirTy::String { attrs: d() });
    assert_eq!(PpirTy::from_type_name("bool"), PpirTy::Bool { attrs: d() });
    assert_eq!(PpirTy::from_type_name("null"), PpirTy::Null { attrs: d() });
    assert_eq!(PpirTy::from_type_name("never"), PpirTy::Never { attrs: d() });
}

#[test]
fn ppir_ty_from_type_name_user_defined() {
    assert_eq!(
        PpirTy::from_type_name("Resume"),
        PpirTy::Named { name: SmolStr::new("Resume"), attrs: d() }
    );
}

#[test]
fn ppir_ty_clone_without_attrs() {
    let ty = PpirTy::Named {
        name: SmolStr::new("Foo"),
        attrs: PpirTypeAttrs {
            stream_done: true,
            stream_with_state: true,
            stream_type: Some(Box::new(PpirTy::Int { attrs: d() })),
        },
    };
    let stripped = ty.clone_without_attrs();
    assert_eq!(
        stripped,
        PpirTy::Named { name: SmolStr::new("Foo"), attrs: d() }
    );
    assert!(stripped.attrs().is_empty());
}

#[test]
fn ppir_type_attrs_is_empty() {
    assert!(PpirTypeAttrs::default().is_empty());
    assert!(!PpirTypeAttrs { stream_done: true, ..Default::default() }.is_empty());
    assert!(!PpirTypeAttrs { stream_with_state: true, ..Default::default() }.is_empty());
    assert!(
        !PpirTypeAttrs {
            stream_type: Some(Box::new(PpirTy::Int { attrs: d() })),
            ..Default::default()
        }
        .is_empty()
    );
}

// ─────────────────────────────── PpirSapMissing tests ────────────────────────

#[test]
fn sap_missing_never_as_ty() {
    assert_eq!(
        PpirSapMissing::Never.as_ty(),
        Some(PpirTy::Never { attrs: d() })
    );
}

#[test]
fn sap_missing_default_as_ty() {
    let ty = PpirTy::Null { attrs: d() };
    assert_eq!(
        PpirSapMissing::Default(ty.clone()).as_ty(),
        Some(ty)
    );
}
