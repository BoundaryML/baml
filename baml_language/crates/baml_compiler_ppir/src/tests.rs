//! Unit tests for PPIR stream expansion and normalization.

use smol_str::SmolStr;

use crate::{
    expand::{default_starts_as, desugar_stream_attrs, expand_stream_class, make_union},
    normalize::{
        StartsAs, StartsAsLiteral, default_starts_as_semantic, infer_typeof_s, normalize_field,
        parse_starts_as_value,
    },
    ty::{ClassifiedField, Ty, PpirTypeRef},
};

// ─────────────────────────────── make_union tests ────────────────────────────

#[test]
fn make_union_both_never() {
    let result = make_union(PpirTypeRef::Never, PpirTypeRef::Never);
    assert_eq!(result, PpirTypeRef::Never);
}

#[test]
fn make_union_s_never() {
    let result = make_union(PpirTypeRef::Never, PpirTypeRef::Int);
    assert_eq!(result, PpirTypeRef::Int);
}

#[test]
fn make_union_d_never() {
    let result = make_union(PpirTypeRef::Int, PpirTypeRef::Never);
    assert_eq!(result, PpirTypeRef::Int);
}

#[test]
fn make_union_same() {
    let result = make_union(PpirTypeRef::Null, PpirTypeRef::Null);
    assert_eq!(result, PpirTypeRef::Null);
}

#[test]
fn make_union_different() {
    let result = make_union(PpirTypeRef::Null, PpirTypeRef::Int);
    assert_eq!(result, PpirTypeRef::union(vec![PpirTypeRef::Null, PpirTypeRef::Int]));
}

// ─────────────────────────────── default_starts_as tests ─────────────────────

#[test]
fn default_starts_as_primitive() {
    assert_eq!(default_starts_as(&PpirTypeRef::Int), PpirTypeRef::Null);
    assert_eq!(default_starts_as(&PpirTypeRef::String), PpirTypeRef::Null);
    assert_eq!(default_starts_as(&PpirTypeRef::Bool), PpirTypeRef::Null);
    assert_eq!(default_starts_as(&PpirTypeRef::Float), PpirTypeRef::Null);
}

#[test]
fn default_starts_as_literal() {
    assert_eq!(
        default_starts_as(&PpirTypeRef::StringLiteral("foo".to_string())),
        PpirTypeRef::Never
    );
    assert_eq!(default_starts_as(&PpirTypeRef::IntLiteral(42)), PpirTypeRef::Never);
    assert_eq!(
        default_starts_as(&PpirTypeRef::BoolLiteral(true)),
        PpirTypeRef::Never
    );
}

#[test]
fn default_starts_as_never() {
    assert_eq!(default_starts_as(&PpirTypeRef::Never), PpirTypeRef::Never);
}

#[test]
fn default_starts_as_null() {
    assert_eq!(default_starts_as(&PpirTypeRef::Null), PpirTypeRef::Null);
}

#[test]
fn default_starts_as_list() {
    let d = PpirTypeRef::list(PpirTypeRef::Int);
    let s = default_starts_as(&d);
    assert_eq!(s, PpirTypeRef::list(PpirTypeRef::Never));
}

#[test]
fn default_starts_as_map() {
    let d = PpirTypeRef::Map {
        key: Box::new(PpirTypeRef::String),
        value: Box::new(PpirTypeRef::Int),
    };
    let s = default_starts_as(&d);
    assert_eq!(
        s,
        PpirTypeRef::Map {
            key: Box::new(PpirTypeRef::String),
            value: Box::new(PpirTypeRef::Never),
        }
    );
}

#[test]
fn default_starts_as_named_type() {
    let d = PpirTypeRef::named(SmolStr::new("stream_Resume"));
    assert_eq!(default_starts_as(&d), PpirTypeRef::Null);
}

// ─────────────────────────────── desugar_stream_attrs tests ──────────────────

fn make_field_with_ty(
    ty: Ty,
    type_ref: PpirTypeRef,
    stream_type: Option<PpirTypeRef>,
    stream_starts_as: Option<String>,
    stream_done: bool,
    stream_not_null: bool,
) -> ClassifiedField {
    ClassifiedField {
        name: SmolStr::new("test_field"),
        ty,
        type_ref,
        stream_type,
        stream_starts_as,
        stream_with_state: false,
        stream_done,
        stream_not_null,
        alias: None,
        description: None,
        skip: false,
    }
}

fn make_field(
    stream_type: Option<PpirTypeRef>,
    stream_starts_as: Option<String>,
    stream_done: bool,
    stream_not_null: bool,
) -> ClassifiedField {
    make_field_with_ty(
        Ty::Primitive(PpirTypeRef::String),
        PpirTypeRef::String,
        stream_type,
        stream_starts_as,
        stream_done,
        stream_not_null,
    )
}

#[test]
fn desugar_no_annotations() {
    let f = make_field(None, None, false, false);
    let (st, hc) = desugar_stream_attrs(&f);
    assert_eq!(st, None);
    assert!(!hc);
}

#[test]
fn desugar_stream_done() {
    let f = make_field(None, None, true, false);
    let (st, hc) = desugar_stream_attrs(&f);
    assert_eq!(st, Some(PpirTypeRef::String));
    assert!(hc);
}

#[test]
fn desugar_stream_not_null() {
    let f = make_field(None, None, false, true);
    let (st, hc) = desugar_stream_attrs(&f);
    // not_null is handled directly in expand_stream_class, not by desugar
    assert_eq!(st, None);
    assert!(!hc);
}

#[test]
fn desugar_stream_done_and_not_null() {
    let f = make_field(None, None, true, true);
    let (st, hc) = desugar_stream_attrs(&f);
    assert_eq!(st, Some(PpirTypeRef::String));
    assert!(hc);
}

#[test]
fn desugar_explicit_stream_type_overrides_done() {
    let f = make_field(Some(PpirTypeRef::Int), None, true, false);
    let (st, hc) = desugar_stream_attrs(&f);
    assert_eq!(st, Some(PpirTypeRef::Int));
    assert!(hc);
}

// ─────────────────────────────── Ty::stream_expand tests ─────────────────────

#[test]
fn stream_expand_primitive() {
    assert_eq!(Ty::Primitive(PpirTypeRef::Int).stream_expand(), PpirTypeRef::Int);
    assert_eq!(
        Ty::Primitive(PpirTypeRef::String).stream_expand(),
        PpirTypeRef::String
    );
    assert_eq!(Ty::Primitive(PpirTypeRef::Bool).stream_expand(), PpirTypeRef::Bool);
    assert_eq!(
        Ty::Primitive(PpirTypeRef::Float).stream_expand(),
        PpirTypeRef::Float
    );
}

#[test]
fn stream_expand_literal() {
    let lit = Ty::Literal(PpirTypeRef::StringLiteral("foo".to_string()));
    assert_eq!(
        lit.stream_expand(),
        PpirTypeRef::StringLiteral("foo".to_string())
    );
}

#[test]
fn stream_expand_null() {
    assert_eq!(Ty::Null.stream_expand(), PpirTypeRef::Null);
}

#[test]
fn stream_expand_never() {
    assert_eq!(Ty::Never.stream_expand(), PpirTypeRef::Never);
}

#[test]
fn stream_expand_class() {
    let ty = Ty::Class(SmolStr::new("Resume"));
    assert_eq!(
        ty.stream_expand(),
        PpirTypeRef::named(SmolStr::new("stream_Resume"))
    );
}

#[test]
fn stream_expand_type_alias() {
    let ty = Ty::TypeAlias(SmolStr::new("MyAlias"));
    assert_eq!(
        ty.stream_expand(),
        PpirTypeRef::named(SmolStr::new("stream_MyAlias"))
    );
}

#[test]
fn stream_expand_enum() {
    let ty = Ty::Enum(SmolStr::new("Color"));
    assert_eq!(ty.stream_expand(), PpirTypeRef::named(SmolStr::new("Color")));
}

#[test]
fn stream_expand_unknown() {
    let ty = Ty::Unknown(PpirTypeRef::named(SmolStr::new("NonExistent")));
    assert_eq!(
        ty.stream_expand(),
        PpirTypeRef::named(SmolStr::new("NonExistent"))
    );
}

#[test]
fn stream_expand_list_of_class() {
    let ty = Ty::List(Box::new(Ty::Class(SmolStr::new("Education"))));
    assert_eq!(
        ty.stream_expand(),
        PpirTypeRef::list(PpirTypeRef::named(SmolStr::new("stream_Education")))
    );
}

#[test]
fn stream_expand_list_of_primitive() {
    let ty = Ty::List(Box::new(Ty::Primitive(PpirTypeRef::Int)));
    assert_eq!(ty.stream_expand(), PpirTypeRef::list(PpirTypeRef::Int));
}

#[test]
fn stream_expand_map() {
    let ty = Ty::Map {
        key: PpirTypeRef::String,
        value: Box::new(Ty::Class(SmolStr::new("Person"))),
    };
    assert_eq!(
        ty.stream_expand(),
        PpirTypeRef::Map {
            key: Box::new(PpirTypeRef::String),
            value: Box::new(PpirTypeRef::named(SmolStr::new("stream_Person"))),
        }
    );
}

#[test]
fn stream_expand_union() {
    let ty = Ty::Union(vec![
        Ty::Class(SmolStr::new("A")),
        Ty::Primitive(PpirTypeRef::Int),
    ]);
    assert_eq!(
        ty.stream_expand(),
        PpirTypeRef::union(vec![PpirTypeRef::named(SmolStr::new("stream_A")), PpirTypeRef::Int,])
    );
}

#[test]
fn stream_expand_optional() {
    let ty = Ty::Optional(Box::new(Ty::Class(SmolStr::new("B"))));
    assert_eq!(
        ty.stream_expand(),
        PpirTypeRef::union(vec![
            PpirTypeRef::named(SmolStr::new("stream_B")),
            PpirTypeRef::Null,
        ])
    );
}

#[test]
fn stream_expand_nested_list() {
    // int[][] → int[][] (primitives pass through)
    let ty = Ty::List(Box::new(Ty::List(Box::new(Ty::Primitive(PpirTypeRef::Int)))));
    assert_eq!(
        ty.stream_expand(),
        PpirTypeRef::list(PpirTypeRef::list(PpirTypeRef::Int))
    );
}

#[test]
fn stream_expand_other() {
    let media = PpirTypeRef::Media(baml_base::MediaKind::Image);
    let ty = Ty::Other(media.clone());
    assert_eq!(ty.stream_expand(), media);
}

// ─────────────────────────────── expand_stream_class tests ───────────────────

#[test]
fn expand_class_primitive_fields() {
    // class Foo { name string, age int }
    // → stream_Foo { name null | string, age null | int }
    let ppir_fields = vec![
        make_field_with_ty(
            Ty::Primitive(PpirTypeRef::String),
            PpirTypeRef::String,
            None,
            None,
            false,
            false,
        ),
        ClassifiedField {
            name: SmolStr::new("age"),
            ty: Ty::Primitive(PpirTypeRef::Int),
            type_ref: PpirTypeRef::Int,
            stream_type: None,
            stream_starts_as: None,
            stream_with_state: false,
            stream_done: false,
            stream_not_null: false,
            alias: None,
            description: None,
            skip: false,
        },
    ];

    let class_name = SmolStr::new("Foo");
    let result = expand_stream_class(&class_name, false, &ppir_fields);
    assert_eq!(result.name.as_str(), "stream_Foo");
    assert_eq!(result.fields.len(), 2);

    // name: null | string
    assert_eq!(result.fields[0].name.as_str(), "test_field"); // from make_field_with_ty
    assert_eq!(
        result.fields[0].type_ref,
        PpirTypeRef::union(vec![PpirTypeRef::Null, PpirTypeRef::String])
    );

    // age: null | int
    assert_eq!(result.fields[1].name.as_str(), "age");
    assert_eq!(
        result.fields[1].type_ref,
        PpirTypeRef::union(vec![PpirTypeRef::Null, PpirTypeRef::Int])
    );
}

#[test]
fn expand_class_with_class_field() {
    // class Resume { education Education }
    // → stream_Resume { education null | stream_Education }
    let ppir_fields = vec![ClassifiedField {
        name: SmolStr::new("education"),
        ty: Ty::Class(SmolStr::new("Education")),
        type_ref: PpirTypeRef::named(SmolStr::new("Education")),
        stream_type: None,
        stream_starts_as: None,
        stream_with_state: false,
        stream_done: false,
        stream_not_null: false,
        alias: None,
        description: None,
        skip: false,
    }];

    let class_name = SmolStr::new("Resume");
    let result = expand_stream_class(&class_name, false, &ppir_fields);
    assert_eq!(result.name.as_str(), "stream_Resume");
    assert_eq!(result.fields.len(), 1);
    assert_eq!(
        result.fields[0].type_ref,
        PpirTypeRef::union(vec![
            PpirTypeRef::Null,
            PpirTypeRef::named(SmolStr::new("stream_Education")),
        ])
    );
}

#[test]
fn expand_class_with_enum_field() {
    // class Resume { status Status }
    // → stream_Resume { status null | Status }
    // Enums are NOT expanded (no stream_ prefix)
    let ppir_fields = vec![ClassifiedField {
        name: SmolStr::new("status"),
        ty: Ty::Enum(SmolStr::new("Status")),
        type_ref: PpirTypeRef::named(SmolStr::new("Status")),
        stream_type: None,
        stream_starts_as: None,
        stream_with_state: false,
        stream_done: false,
        stream_not_null: false,
        alias: None,
        description: None,
        skip: false,
    }];

    let class_name = SmolStr::new("Resume");
    let result = expand_stream_class(&class_name, false, &ppir_fields);
    assert_eq!(result.fields[0].name.as_str(), "status");
    assert_eq!(
        result.fields[0].type_ref,
        PpirTypeRef::union(vec![PpirTypeRef::Null, PpirTypeRef::named(SmolStr::new("Status")),])
    );
}

#[test]
fn expand_class_with_list_of_classes() {
    // class Resume { education Education[] }
    // → stream_Resume { education never[] | stream_Education[] }
    let ppir_fields = vec![ClassifiedField {
        name: SmolStr::new("education"),
        ty: Ty::List(Box::new(Ty::Class(SmolStr::new("Education")))),
        type_ref: PpirTypeRef::list(PpirTypeRef::named(SmolStr::new("Education"))),
        stream_type: None,
        stream_starts_as: None,
        stream_with_state: false,
        stream_done: false,
        stream_not_null: false,
        alias: None,
        description: None,
        skip: false,
    }];

    let class_name = SmolStr::new("Resume");
    let result = expand_stream_class(&class_name, false, &ppir_fields);
    assert_eq!(result.fields[0].name.as_str(), "education");
    // D = stream_Education[], S = never[] (empty list)
    // S | D = never[] | stream_Education[]
    assert_eq!(
        result.fields[0].type_ref,
        PpirTypeRef::union(vec![
            PpirTypeRef::list(PpirTypeRef::Never),
            PpirTypeRef::list(PpirTypeRef::named(SmolStr::new("stream_Education"))),
        ])
    );
}

#[test]
fn expand_class_field_omission() {
    // A literal field with @stream.not_null: D=literal (never in this case), S=never
    // never | never → never → field omitted
    let ppir_fields = vec![ClassifiedField {
        name: SmolStr::new("kind"),
        ty: Ty::Literal(PpirTypeRef::StringLiteral("resume".to_string())),
        type_ref: PpirTypeRef::StringLiteral("resume".to_string()),
        stream_type: None,
        stream_starts_as: None,
        stream_with_state: false,
        stream_done: false,
        stream_not_null: true,
        alias: None,
        description: None,
        skip: false,
    }];

    let class_name = SmolStr::new("Foo");
    let result = expand_stream_class(&class_name, false, &ppir_fields);
    // Literal type: D = "resume" (unchanged), S = never (from @stream.not_null)
    // make_union(never, "resume") → "resume", so field is NOT omitted
    // Only omitted when both S=never AND D=never
    assert_eq!(result.fields.len(), 1);
    assert_eq!(
        result.fields[0].type_ref,
        PpirTypeRef::StringLiteral("resume".to_string())
    );
}

#[test]
fn expand_class_never_field_omission() {
    // If a field has type never and @stream.not_null, both D and S are never → omit
    let ppir_fields = vec![ClassifiedField {
        name: SmolStr::new("phantom"),
        ty: Ty::Never,
        type_ref: PpirTypeRef::Never,
        stream_type: None,
        stream_starts_as: None,
        stream_with_state: false,
        stream_done: false,
        stream_not_null: true, // S = never
        alias: None,
        description: None,
        skip: false,
    }];

    let class_name = SmolStr::new("Foo");
    let result = expand_stream_class(&class_name, false, &ppir_fields);
    // D = never (from Ty::Never.stream_expand()), S = never (from @stream.not_null)
    // Both never → field omitted
    assert_eq!(result.fields.len(), 0);
}

#[test]
fn expand_class_stream_done_field() {
    // @stream.done on a string field → D = string (original type), S = null (default)
    let ppir_fields = vec![ClassifiedField {
        name: SmolStr::new("name"),
        ty: Ty::Primitive(PpirTypeRef::String),
        type_ref: PpirTypeRef::String,
        stream_type: None,
        stream_starts_as: None,
        stream_with_state: false,
        stream_done: true,
        stream_not_null: false,
        alias: None,
        description: None,
        skip: false,
    }];

    let class_name = SmolStr::new("Foo");
    let result = expand_stream_class(&class_name, false, &ppir_fields);
    // @stream.done → D = string (original type, not stream-expanded)
    // S = null (default for string)
    // Result: null | string
    assert_eq!(
        result.fields[0].type_ref,
        PpirTypeRef::union(vec![PpirTypeRef::Null, PpirTypeRef::String])
    );
}

#[test]
fn expand_class_carries_through_attributes() {
    let field = ClassifiedField {
        name: SmolStr::new("name"),
        ty: Ty::Primitive(PpirTypeRef::String),
        type_ref: PpirTypeRef::String,
        stream_type: None,
        stream_starts_as: None,
        stream_with_state: false,
        stream_done: false,
        stream_not_null: false,
        alias: Some("my_alias".to_string()),
        description: Some("my desc".to_string()),
        skip: true,
    };

    let class_name = SmolStr::new("Foo");
    let result = expand_stream_class(&class_name, false, &[field]);
    assert_eq!(result.fields[0].alias, Some("my_alias".to_string()));
    assert_eq!(result.fields[0].description, Some("my desc".to_string()));
    assert!(result.fields[0].skip);
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
    assert_eq!(default_starts_as_semantic(&PpirTypeRef::Int), StartsAs::Null);
    assert_eq!(
        default_starts_as_semantic(&PpirTypeRef::String),
        StartsAs::Null
    );
    assert_eq!(
        default_starts_as_semantic(&PpirTypeRef::Bool),
        StartsAs::Null
    );
    assert_eq!(
        default_starts_as_semantic(&PpirTypeRef::Float),
        StartsAs::Null
    );
}

#[test]
fn default_starts_as_semantic_literal() {
    assert_eq!(
        default_starts_as_semantic(&PpirTypeRef::StringLiteral("foo".to_string())),
        StartsAs::Never
    );
    assert_eq!(
        default_starts_as_semantic(&PpirTypeRef::IntLiteral(42)),
        StartsAs::Never
    );
    assert_eq!(
        default_starts_as_semantic(&PpirTypeRef::BoolLiteral(true)),
        StartsAs::Never
    );
}

#[test]
fn default_starts_as_semantic_list() {
    assert_eq!(
        default_starts_as_semantic(&PpirTypeRef::list(PpirTypeRef::Int)),
        StartsAs::EmptyList
    );
}

#[test]
fn default_starts_as_semantic_map() {
    let d = PpirTypeRef::Map {
        key: Box::new(PpirTypeRef::String),
        value: Box::new(PpirTypeRef::Int),
    };
    assert_eq!(default_starts_as_semantic(&d), StartsAs::EmptyMap);
}

#[test]
fn default_starts_as_semantic_named() {
    assert_eq!(
        default_starts_as_semantic(&PpirTypeRef::named("stream_Resume")),
        StartsAs::Null
    );
}

#[test]
fn default_starts_as_semantic_never() {
    assert_eq!(
        default_starts_as_semantic(&PpirTypeRef::Never),
        StartsAs::Never
    );
}

#[test]
fn default_starts_as_semantic_null() {
    assert_eq!(
        default_starts_as_semantic(&PpirTypeRef::Null),
        StartsAs::Null
    );
}

// ─────────────────────────────── infer_typeof_s tests ─────────────────────────

#[test]
fn infer_typeof_s_never() {
    assert_eq!(infer_typeof_s(&StartsAs::Never), Some(PpirTypeRef::Never));
}

#[test]
fn infer_typeof_s_null() {
    assert_eq!(infer_typeof_s(&StartsAs::Null), Some(PpirTypeRef::Null));
}

#[test]
fn infer_typeof_s_string_literal() {
    assert_eq!(
        infer_typeof_s(&StartsAs::Literal(StartsAsLiteral::String(
            "Loading...".to_string()
        ))),
        Some(PpirTypeRef::StringLiteral("Loading...".to_string()))
    );
}

#[test]
fn infer_typeof_s_int_literal() {
    assert_eq!(
        infer_typeof_s(&StartsAs::Literal(StartsAsLiteral::Int(0))),
        Some(PpirTypeRef::IntLiteral(0))
    );
}

#[test]
fn infer_typeof_s_bool_literal() {
    assert_eq!(
        infer_typeof_s(&StartsAs::Literal(StartsAsLiteral::Bool(false))),
        Some(PpirTypeRef::BoolLiteral(false))
    );
}

#[test]
fn infer_typeof_s_float_literal() {
    assert_eq!(
        infer_typeof_s(&StartsAs::Literal(StartsAsLiteral::Float(
            "3.14".to_string()
        ))),
        Some(PpirTypeRef::Float)
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

// ─────────────────────────────── normalize_field tests ────────────────────────

#[test]
fn normalize_primitive_field_defaults() {
    // name string → D=string, S=null, typeof_s=null
    let f = make_field(None, None, false, false);
    let n = normalize_field(&f);
    assert_eq!(n.stream_type, PpirTypeRef::String);
    assert_eq!(n.starts_as, StartsAs::Null);
    assert_eq!(n.typeof_s, Some(PpirTypeRef::Null));
    assert!(!n.in_progress_never);
}

#[test]
fn normalize_class_field_defaults() {
    // edu Education → D=stream_Education, S=null
    let f = make_field_with_ty(
        Ty::Class(SmolStr::new("Education")),
        PpirTypeRef::named("Education"),
        None,
        None,
        false,
        false,
    );
    let n = normalize_field(&f);
    assert_eq!(
        n.stream_type,
        PpirTypeRef::named(SmolStr::new("stream_Education"))
    );
    assert_eq!(n.starts_as, StartsAs::Null);
    assert_eq!(n.typeof_s, Some(PpirTypeRef::Null));
}

#[test]
fn normalize_list_field_defaults() {
    // scores int[] → D=int[], S=[]
    let f = make_field_with_ty(
        Ty::List(Box::new(Ty::Primitive(PpirTypeRef::Int))),
        PpirTypeRef::list(PpirTypeRef::Int),
        None,
        None,
        false,
        false,
    );
    let n = normalize_field(&f);
    assert_eq!(n.stream_type, PpirTypeRef::list(PpirTypeRef::Int));
    assert_eq!(n.starts_as, StartsAs::EmptyList);
    assert_eq!(n.typeof_s, None); // deferred
}

#[test]
fn normalize_map_field_defaults() {
    // m map<string, Education> → D=map<string, stream_Education>, S={}
    let f = make_field_with_ty(
        Ty::Map {
            key: PpirTypeRef::String,
            value: Box::new(Ty::Class(SmolStr::new("Education"))),
        },
        PpirTypeRef::Map {
            key: Box::new(PpirTypeRef::String),
            value: Box::new(PpirTypeRef::named("Education")),
        },
        None,
        None,
        false,
        false,
    );
    let n = normalize_field(&f);
    assert_eq!(
        n.stream_type,
        PpirTypeRef::Map {
            key: Box::new(PpirTypeRef::String),
            value: Box::new(PpirTypeRef::named(SmolStr::new("stream_Education"))),
        }
    );
    assert_eq!(n.starts_as, StartsAs::EmptyMap);
    assert_eq!(n.typeof_s, None); // deferred
}

#[test]
fn normalize_literal_field_defaults() {
    // type "resume" → D="resume", S=never
    let f = make_field_with_ty(
        Ty::Literal(PpirTypeRef::StringLiteral("resume".to_string())),
        PpirTypeRef::StringLiteral("resume".to_string()),
        None,
        None,
        false,
        false,
    );
    let n = normalize_field(&f);
    assert_eq!(
        n.stream_type,
        PpirTypeRef::StringLiteral("resume".to_string())
    );
    assert_eq!(n.starts_as, StartsAs::Never);
    assert_eq!(n.typeof_s, Some(PpirTypeRef::Never));
}

#[test]
fn normalize_stream_done() {
    // name string @stream.done → D=string (original), in_progress_never=true, S=null
    let f = make_field(None, None, true, false);
    let n = normalize_field(&f);
    assert_eq!(n.stream_type, PpirTypeRef::String);
    assert!(n.in_progress_never);
    assert_eq!(n.starts_as, StartsAs::Null);
}

#[test]
fn normalize_stream_not_null() {
    // gpa float @stream.not_null → D=float, S=never
    let f = make_field_with_ty(
        Ty::Primitive(PpirTypeRef::Float),
        PpirTypeRef::Float,
        None,
        None,
        false,
        true,
    );
    let n = normalize_field(&f);
    assert_eq!(n.stream_type, PpirTypeRef::Float);
    assert_eq!(n.starts_as, StartsAs::Never);
    assert_eq!(n.typeof_s, Some(PpirTypeRef::Never));
}

#[test]
fn normalize_stream_done_and_not_null() {
    // id string @stream.done @stream.not_null → D=string, in_progress_never=true, S=never
    let f = make_field(None, None, true, true);
    let n = normalize_field(&f);
    assert_eq!(n.stream_type, PpirTypeRef::String);
    assert!(n.in_progress_never);
    assert_eq!(n.starts_as, StartsAs::Never);
}

#[test]
fn normalize_explicit_stream_type() {
    // certifications Education[] @stream.type(Education[])
    // → D=Education[] (explicit, no stream_ prefix), S=[] (list default)
    let f = make_field_with_ty(
        Ty::List(Box::new(Ty::Class(SmolStr::new("Education")))),
        PpirTypeRef::list(PpirTypeRef::named("Education")),
        Some(PpirTypeRef::list(PpirTypeRef::named("Education"))),
        None,
        false,
        false,
    );
    let n = normalize_field(&f);
    assert_eq!(
        n.stream_type,
        PpirTypeRef::list(PpirTypeRef::named("Education"))
    );
    assert_eq!(n.starts_as, StartsAs::EmptyList);
}

#[test]
fn normalize_explicit_starts_as_string() {
    // name string @stream.starts_as("Loading...")
    // → D=string (default), S="Loading..."
    let f = make_field(None, Some("Loading...".to_string()), false, false);
    let n = normalize_field(&f);
    assert_eq!(n.stream_type, PpirTypeRef::String);
    assert_eq!(
        n.starts_as,
        StartsAs::Literal(StartsAsLiteral::String("Loading...".to_string()))
    );
    assert_eq!(
        n.typeof_s,
        Some(PpirTypeRef::StringLiteral("Loading...".to_string()))
    );
}

#[test]
fn normalize_explicit_starts_as_never() {
    // title string @stream.type(string) @stream.starts_as(never)
    // → D=string, S=never
    let f = make_field(Some(PpirTypeRef::String), Some("never".to_string()), false, false);
    let n = normalize_field(&f);
    assert_eq!(n.stream_type, PpirTypeRef::String);
    assert_eq!(n.starts_as, StartsAs::Never);
    assert_eq!(n.typeof_s, Some(PpirTypeRef::Never));
}

#[test]
fn normalize_optional_field_defaults() {
    // maybe string? → D=string | null, S=null
    let f = make_field_with_ty(
        Ty::Optional(Box::new(Ty::Primitive(PpirTypeRef::String))),
        PpirTypeRef::optional(PpirTypeRef::String),
        None,
        None,
        false,
        false,
    );
    let n = normalize_field(&f);
    // Optional stream_expand: inner.stream_expand() | null
    assert_eq!(
        n.stream_type,
        PpirTypeRef::union(vec![PpirTypeRef::String, PpirTypeRef::Null])
    );
    assert_eq!(n.starts_as, StartsAs::Null);
}

#[test]
fn normalize_optional_list_field_defaults() {
    // maybe_list int[]? → D=int[] | null, S=null (NOT [])
    let f = make_field_with_ty(
        Ty::Optional(Box::new(Ty::List(Box::new(Ty::Primitive(PpirTypeRef::Int))))),
        PpirTypeRef::optional(PpirTypeRef::list(PpirTypeRef::Int)),
        None,
        None,
        false,
        false,
    );
    let n = normalize_field(&f);
    // Optional: D = int[] | null (union, not a list)
    assert_eq!(
        n.stream_type,
        PpirTypeRef::union(vec![PpirTypeRef::list(PpirTypeRef::Int), PpirTypeRef::Null])
    );
    // Top-level is a union, not a list → S=null, NOT []
    assert_eq!(n.starts_as, StartsAs::Null);
}

#[test]
fn normalize_stream_type_never() {
    // internal string @stream.type(never) → D=never, S=never
    let f = make_field(Some(PpirTypeRef::Never), None, false, false);
    let n = normalize_field(&f);
    assert_eq!(n.stream_type, PpirTypeRef::Never);
    assert_eq!(n.starts_as, StartsAs::Never);
    assert_eq!(n.typeof_s, Some(PpirTypeRef::Never));
}
