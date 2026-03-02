//! Unit tests for PPIR stream expansion and normalization.

use baml_base::Name;
use rustc_hash::FxHashMap;
use smol_str::SmolStr;

use crate::{
    desugar::{PpirStreamStartsAs, default_sap_starts_as},
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

// ─────────────────────────────── default_sap_starts_as tests ─────────────────

#[test]
fn default_sap_starts_as_primitive() {
    match default_sap_starts_as(&PpirTy::Int { attrs: d() }) {
        PpirStreamStartsAs::DefaultFor(ty) => assert_eq!(ty, PpirTy::Null { attrs: d() }),
        other => panic!("expected DefaultFor, got {other:?}"),
    }
}

#[test]
fn default_sap_starts_as_literal() {
    assert_eq!(
        default_sap_starts_as(&PpirTy::StringLiteral {
            value: "foo".to_string(),
            attrs: d()
        }),
        PpirStreamStartsAs::Never
    );
}

#[test]
fn default_sap_starts_as_never() {
    assert_eq!(
        default_sap_starts_as(&PpirTy::Never { attrs: d() }),
        PpirStreamStartsAs::Never
    );
}

#[test]
fn default_sap_starts_as_list() {
    match default_sap_starts_as(&PpirTy::list(PpirTy::Int { attrs: d() })) {
        PpirStreamStartsAs::DefaultFor(ty) => {
            assert_eq!(ty, PpirTy::list(PpirTy::Never { attrs: d() }));
        }
        other => panic!("expected DefaultFor, got {other:?}"),
    }
}

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
    assert_eq!(
        default_starts_as_semantic(&PpirTy::Int { attrs: d() }),
        StartsAs::Null
    );
    assert_eq!(
        default_starts_as_semantic(&PpirTy::String { attrs: d() }),
        StartsAs::Null
    );
    assert_eq!(
        default_starts_as_semantic(&PpirTy::Bool { attrs: d() }),
        StartsAs::Null
    );
    assert_eq!(
        default_starts_as_semantic(&PpirTy::Float { attrs: d() }),
        StartsAs::Null
    );
}

#[test]
fn default_starts_as_semantic_literal() {
    assert_eq!(
        default_starts_as_semantic(&PpirTy::StringLiteral {
            value: "foo".to_string(),
            attrs: d()
        }),
        StartsAs::Never
    );
    assert_eq!(
        default_starts_as_semantic(&PpirTy::IntLiteral {
            value: 42,
            attrs: d()
        }),
        StartsAs::Never
    );
    assert_eq!(
        default_starts_as_semantic(&PpirTy::BoolLiteral {
            value: true,
            attrs: d()
        }),
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

fn empty_enum_names() -> FxHashMap<Name, Vec<Name>> {
    FxHashMap::default()
}

fn enum_names_with(names: &[&str]) -> FxHashMap<Name, Vec<Name>> {
    names
        .iter()
        .map(|n| (SmolStr::new(n), Vec::new()))
        .collect()
}

#[test]
fn infer_typeof_s_never() {
    assert_eq!(
        infer_typeof_s(&StartsAs::Never, &empty_enum_names()),
        Some(PpirTy::Never { attrs: d() })
    );
}

#[test]
fn infer_typeof_s_null() {
    assert_eq!(
        infer_typeof_s(&StartsAs::Null, &empty_enum_names()),
        Some(PpirTy::Null { attrs: d() })
    );
}

#[test]
fn infer_typeof_s_string_literal() {
    assert_eq!(
        infer_typeof_s(
            &StartsAs::Literal(StartsAsLiteral::String("Loading...".to_string())),
            &empty_enum_names()
        ),
        Some(PpirTy::StringLiteral {
            value: "Loading...".to_string(),
            attrs: d()
        })
    );
}

#[test]
fn infer_typeof_s_int_literal() {
    assert_eq!(
        infer_typeof_s(
            &StartsAs::Literal(StartsAsLiteral::Int(0)),
            &empty_enum_names()
        ),
        Some(PpirTy::IntLiteral {
            value: 0,
            attrs: d()
        })
    );
}

#[test]
fn infer_typeof_s_bool_literal() {
    assert_eq!(
        infer_typeof_s(
            &StartsAs::Literal(StartsAsLiteral::Bool(false)),
            &empty_enum_names()
        ),
        Some(PpirTy::BoolLiteral {
            value: false,
            attrs: d()
        })
    );
}

#[test]
fn infer_typeof_s_float_literal() {
    assert_eq!(
        infer_typeof_s(
            &StartsAs::Literal(StartsAsLiteral::Float("3.14".to_string())),
            &empty_enum_names()
        ),
        Some(PpirTy::Float { attrs: d() })
    );
}

#[test]
fn infer_typeof_s_empty_list() {
    assert_eq!(
        infer_typeof_s(&StartsAs::EmptyList, &empty_enum_names()),
        None
    );
}

#[test]
fn infer_typeof_s_empty_map() {
    assert_eq!(
        infer_typeof_s(&StartsAs::EmptyMap, &empty_enum_names()),
        None
    );
}

// ─────────────────────────────── enum value tests ────────────────────────────

#[test]
fn parse_starts_as_enum_value() {
    assert_eq!(
        parse_starts_as_value("Status.Loading"),
        StartsAs::EnumValue {
            enum_name: "Status".to_string(),
            variant_name: "Loading".to_string(),
        }
    );
}

#[test]
fn parse_starts_as_single_identifier_is_string() {
    assert_eq!(
        parse_starts_as_value("foo"),
        StartsAs::Literal(StartsAsLiteral::String("foo".to_string()))
    );
}

#[test]
fn parse_starts_as_float_not_enum() {
    // "3.14" should be parsed as float, not enum value
    assert_eq!(
        parse_starts_as_value("3.14"),
        StartsAs::Literal(StartsAsLiteral::Float("3.14".to_string()))
    );
}

#[test]
fn infer_typeof_s_enum_value_known() {
    let enums = enum_names_with(&["Status"]);
    assert_eq!(
        infer_typeof_s(
            &StartsAs::EnumValue {
                enum_name: "Status".to_string(),
                variant_name: "Loading".to_string(),
            },
            &enums,
        ),
        Some(PpirTy::Named {
            name: SmolStr::new("Status"),
            attrs: d()
        })
    );
}

#[test]
fn infer_typeof_s_enum_value_unknown() {
    assert_eq!(
        infer_typeof_s(
            &StartsAs::EnumValue {
                enum_name: "Unknown".to_string(),
                variant_name: "X".to_string(),
            },
            &empty_enum_names(),
        ),
        None
    );
}

#[test]
fn infer_typeof_s_unknown() {
    assert_eq!(
        infer_typeof_s(
            &StartsAs::Unknown("foo(bar)".to_string()),
            &empty_enum_names(),
        ),
        None
    );
}

// ─────────────────────────────── PpirTy constructor tests ────────────────────

#[test]
fn ppir_ty_from_type_name_primitives() {
    assert_eq!(PpirTy::from_type_name("int"), PpirTy::Int { attrs: d() });
    assert_eq!(
        PpirTy::from_type_name("float"),
        PpirTy::Float { attrs: d() }
    );
    assert_eq!(
        PpirTy::from_type_name("string"),
        PpirTy::String { attrs: d() }
    );
    assert_eq!(PpirTy::from_type_name("bool"), PpirTy::Bool { attrs: d() });
    assert_eq!(PpirTy::from_type_name("null"), PpirTy::Null { attrs: d() });
    assert_eq!(
        PpirTy::from_type_name("never"),
        PpirTy::Never { attrs: d() }
    );
}

#[test]
fn ppir_ty_from_type_name_user_defined() {
    assert_eq!(
        PpirTy::from_type_name("Resume"),
        PpirTy::Named {
            name: SmolStr::new("Resume"),
            attrs: d()
        }
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
        PpirTy::Named {
            name: SmolStr::new("Foo"),
            attrs: d()
        }
    );
    assert!(stripped.attrs().is_empty());
}

#[test]
fn ppir_type_attrs_is_empty() {
    assert!(PpirTypeAttrs::default().is_empty());
    assert!(
        !PpirTypeAttrs {
            stream_done: true,
            ..Default::default()
        }
        .is_empty()
    );
    assert!(
        !PpirTypeAttrs {
            stream_with_state: true,
            ..Default::default()
        }
        .is_empty()
    );
    assert!(
        !PpirTypeAttrs {
            stream_type: Some(Box::new(PpirTy::Int { attrs: d() })),
            ..Default::default()
        }
        .is_empty()
    );
}

// ─────────────────────────────── PpirStreamStartsAs tests ────────────────────

#[test]
fn sap_starts_as_never_as_ty() {
    assert_eq!(
        PpirStreamStartsAs::Never.as_ty(),
        Some(PpirTy::Never { attrs: d() })
    );
}

#[test]
fn sap_starts_as_default_as_ty() {
    let ty = PpirTy::Null { attrs: d() };
    assert_eq!(PpirStreamStartsAs::DefaultFor(ty.clone()).as_ty(), Some(ty));
}
