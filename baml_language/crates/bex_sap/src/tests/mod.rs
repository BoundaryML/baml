#![cfg(test)]
#[macro_use]
pub mod macros;

mod test_aliases;
mod test_basics;
mod test_class;
mod test_class_2;
mod test_code;
// mod test_constraints; // Assertion::evaluate is todo!()
mod test_enum;
mod test_international;
mod test_lists;
mod test_literals;
mod test_maps;
mod test_partials;
mod test_streaming;
mod test_unions;
// mod animation; // Depends on constraint/streaming infra

use std::borrow::Cow;

use crate::sap_model::*;

// ---------------------------------------------------------------------------
// Helper functions to construct sap_model types ergonomically in tests.
// All helpers use `&'static str` as the type identifier `N`.
// ---------------------------------------------------------------------------

/// Wrap a `TyResolved` in an `AnnotatedTy` with default annotations.
fn annotated(ty: impl Into<Ty<'static, &'static str>>) -> AnnotatedTy<'static, &'static str> {
    TyWithMeta::new(ty.into(), TypeAnnotations::default())
}

/// Wrap a `TyResolved` in an `AnnotatedTy` with custom annotations.
fn annotated_with(
    ty: impl Into<Ty<'static, &'static str>>,
    meta: TypeAnnotations<'static, &'static str>,
) -> AnnotatedTy<'static, &'static str> {
    TyWithMeta::new(ty.into(), meta)
}

// --- Primitive type constructors ---

fn string_ty() -> TyResolved<'static, &'static str> {
    TyResolved::String(StringTy)
}

fn int_ty() -> TyResolved<'static, &'static str> {
    TyResolved::Int(IntTy)
}

fn float_ty() -> TyResolved<'static, &'static str> {
    TyResolved::Float(FloatTy)
}

fn bool_ty() -> TyResolved<'static, &'static str> {
    TyResolved::Bool(BoolTy)
}

fn null_ty() -> TyResolved<'static, &'static str> {
    TyResolved::Null(NullTy)
}

// --- Literal type constructors ---

fn literal_int(v: i64) -> TyResolved<'static, &'static str> {
    TyResolved::LiteralInt(IntLiteralTy(v))
}

fn literal_bool(v: bool) -> TyResolved<'static, &'static str> {
    TyResolved::LiteralBool(BoolLiteralTy(v))
}

fn literal_string(v: &'static str) -> TyResolved<'static, &'static str> {
    TyResolved::LiteralString(StringLiteralTy(Cow::Borrowed(v)))
}

fn literal_string_owned(v: String) -> TyResolved<'static, &'static str> {
    TyResolved::LiteralString(StringLiteralTy(Cow::Owned(v)))
}

// --- Composite type constructors ---

fn array_of(item: AnnotatedTy<'static, &'static str>) -> TyResolved<'static, &'static str> {
    TyResolved::Array(ArrayTy { ty: Box::new(item) })
}

fn map_of(
    key: AnnotatedTy<'static, &'static str>,
    value: AnnotatedTy<'static, &'static str>,
) -> TyResolved<'static, &'static str> {
    TyResolved::Map(MapTy {
        key: Box::new(key),
        value: Box::new(value),
    })
}

fn union_of(
    variants: Vec<AnnotatedTy<'static, &'static str>>,
) -> TyResolved<'static, &'static str> {
    TyResolved::Union(UnionTy { variants })
}

fn optional(inner: TyResolved<'static, &'static str>) -> TyResolved<'static, &'static str> {
    union_of(vec![annotated(inner), annotated(null_ty())])
}

fn class_ty(
    name: &'static str,
    fields: Vec<AnnotatedField<'static, &'static str>>,
) -> TyResolved<'static, &'static str> {
    TyResolved::Class(ClassTy { name, fields })
}

fn enum_ty(
    name: &'static str,
    variants: Vec<AnnotatedEnumVariant<'static>>,
) -> TyResolved<'static, &'static str> {
    TyResolved::Enum(EnumTy { name, variants })
}

fn stream_state_ty(inner: AnnotatedTy<'static, &'static str>) -> TyResolved<'static, &'static str> {
    TyResolved::StreamState(StreamStateTy {
        value: Box::new(inner),
    })
}

// --- Field constructors ---

/// Create a required field (missing = never, before_started = null for streaming).
fn field(
    name: &'static str,
    ty: impl Into<Ty<'static, &'static str>>,
) -> AnnotatedField<'static, &'static str> {
    AnnotatedField {
        name: Cow::Borrowed(name),
        ty: annotated(ty),
        class_in_progress_field_missing: AttrLiteral::Null,
        class_completed_field_missing: AttrLiteral::Never,
        aliases: vec![],
    }
}

/// Create an optional field (missing = null, before_started = null).
fn optional_field(
    name: &'static str,
    inner_ty: TyResolved<'static, &'static str>,
) -> AnnotatedField<'static, &'static str> {
    AnnotatedField {
        name: Cow::Borrowed(name),
        ty: annotated(optional(inner_ty)),
        class_in_progress_field_missing: AttrLiteral::Null,
        class_completed_field_missing: AttrLiteral::Null,
        aliases: vec![],
    }
}

/// Create a field with aliases.
fn field_with_aliases(
    name: &'static str,
    ty: impl Into<Ty<'static, &'static str>>,
    aliases: Vec<&'static str>,
) -> AnnotatedField<'static, &'static str> {
    AnnotatedField {
        name: Cow::Borrowed(name),
        ty: annotated(ty),
        class_in_progress_field_missing: AttrLiteral::Null,
        class_completed_field_missing: AttrLiteral::Never,
        aliases: aliases.into_iter().map(Cow::Borrowed).collect(),
    }
}

/// Create an optional field with aliases.
fn optional_field_with_aliases(
    name: &'static str,
    inner_ty: TyResolved<'static, &'static str>,
    aliases: Vec<&'static str>,
) -> AnnotatedField<'static, &'static str> {
    AnnotatedField {
        name: Cow::Borrowed(name),
        ty: annotated(optional(inner_ty)),
        class_in_progress_field_missing: AttrLiteral::Null,
        class_completed_field_missing: AttrLiteral::Null,
        aliases: aliases.into_iter().map(Cow::Borrowed).collect(),
    }
}

// --- Enum variant constructors ---

fn variant(name: &'static str) -> AnnotatedEnumVariant<'static> {
    AnnotatedEnumVariant {
        name: Cow::Borrowed(name),
        aliases: vec![],
    }
}

fn variant_with_aliases(
    name: &'static str,
    aliases: Vec<&'static str>,
) -> AnnotatedEnumVariant<'static> {
    AnnotatedEnumVariant {
        name: Cow::Borrowed(name),
        aliases: aliases.into_iter().map(Cow::Borrowed).collect(),
    }
}

fn variant_with_owned_aliases(
    name: &'static str,
    aliases: Vec<String>,
) -> AnnotatedEnumVariant<'static> {
    AnnotatedEnumVariant {
        name: Cow::Borrowed(name),
        aliases: aliases.into_iter().map(Cow::Owned).collect(),
    }
}

/// An empty TypeRefDb.
fn empty_db() -> TypeRefDb<'static, &'static str> {
    TypeRefDb::new()
}

// ---------------------------------------------------------------------------
// Inline tests from old mod.rs (primitives + simple classes)
// ---------------------------------------------------------------------------

test_deserializer!(
    test_string_from_string,
    r#"hello"#,
    string_ty(),
    empty_db(),
    "hello"
);

test_deserializer!(
    test_string_from_string_with_quotes,
    r#""hello""#,
    string_ty(),
    empty_db(),
    "\"hello\""
);

test_deserializer!(
    test_string_from_object,
    r#"{"hi":    "hello"}"#,
    string_ty(),
    empty_db(),
    r#"{"hi":    "hello"}"#
);

test_deserializer!(
    test_string_from_obj_and_string,
    r#"The output is: {"hello": "world"}"#,
    string_ty(),
    empty_db(),
    "The output is: {\"hello\": \"world\"}"
);

test_deserializer!(
    test_string_from_list,
    r#"["hello", "world"]"#,
    string_ty(),
    empty_db(),
    "[\"hello\", \"world\"]"
);

test_deserializer!(test_string_from_int, r#"1"#, string_ty(), empty_db(), "1");

test_deserializer!(
    test_string_from_string21,
    r#"Some preview text

    JSON Output:

    [
      {
        "blah": "blah"
      },
      {
        "blah": "blah"
      },
      {
        "blah": "blah"
      }
    ]"#,
    string_ty(),
    empty_db(),
    r#"Some preview text

    JSON Output:

    [
      {
        "blah": "blah"
      },
      {
        "blah": "blah"
      },
      {
        "blah": "blah"
      }
    ]"#
);

test_deserializer!(
    test_string_from_string22,
    r#"Hello there.

    JSON Output:
    ```json
    [
      {
        "id": "hi"
      },
      {
        "id": "hi"
      },
      {
        "id": "hi"
      }
    ]
    ```
    "#,
    string_ty(),
    empty_db(),
    r#"Hello there.

    JSON Output:
    ```json
    [
      {
        "id": "hi"
      },
      {
        "id": "hi"
      },
      {
        "id": "hi"
      }
    ]
    ```
    "#
);

test_deserializer!(
    test_string_from_string23,
    r#"Hello there. Here is {{playername}

  JSON Output:

    {
      "id": "{{hi} there"
    }

  "#,
    class_ty("Foo", vec![optional_field("id", string_ty())]),
    empty_db(),
    {"id": null }
);

test_deserializer!(
    test_string_from_string24,
    r#"Hello there. Here is {playername}

    JSON Output:

      {
        "id": "{{hi} there",
      }

    "#,
    class_ty("Foo", vec![optional_field("id", string_ty())]),
    empty_db(),
    {"id": r#"{{hi} there"# }
);

// BookAnalysis example
#[test]
fn test_string_from_string25() {
    let db = crate::baml_db! {
        class Score { year: int, score: int }
        class PopularityOverTime { bookName: string, scores: [Score] }
    };
    // BookAnalysis target uses field_with_aliases, so it can't go in baml_db!
    let book_analysis = class_ty(
        "BookAnalysis",
        vec![
            field("bookNames", array_of(annotated(string_ty()))),
            field_with_aliases(
                "popularityOverTime",
                array_of(annotated(Ty::Unresolved("PopularityOverTime"))),
                vec!["popularityData"],
            ),
        ],
    );

    let raw = r#"
    {
        "bookNames": ["brave new world", "the lord of the rings", "three body problem", "stormlight archive"],
        "popularityData": [
          {
            "bookName": "brave new world",
            "scores": [
              {"year": 1932, "score": 65},
              {"year": 2000, "score": 80},
              {"year": 2021, "score": 70}
            ]
          },
          {
            "bookName": "the lord of the rings",
            "scores": [
              {"year": 1954, "score": 75},
              {"year": 2001, "score": 95},
              {"year": 2021, "score": 90}
            ]
          },
          {
            "bookName": "three body problem",
            "scores": [
              {"year": 2008, "score": 60},
              {"year": 2014, "score": 79},
              {"year": 2021, "score": 85}
            ]
          },
          {
            "bookName": "stormlight archive",
            "scores": [
              {"year": 2010, "score": 76},
              {"year": 2020, "score": 85},
              {"year": 2021, "score": 81}
            ]
          }
        ]
      }
    "#;

    let parsed = crate::jsonish::parse(raw, Default::default(), true).unwrap();
    let ctx = crate::deserializer::coercer::ParsingContext::new(book_analysis.as_ref(), &db);
    let default_annotations = TypeAnnotations::default();
    let target = TyWithMeta::new(book_analysis.as_ref(), &default_annotations);
    let result = TyResolvedRef::coerce(&ctx, target, &parsed);
    assert!(result.is_ok(), "Failed to parse: {:?}", result);
    let value = result.unwrap().unwrap();
    let json_value = serde_json::to_value(&value).unwrap();
    let expected = serde_json::json!({
      "bookNames": ["brave new world", "the lord of the rings", "three body problem", "stormlight archive"],
      "popularityOverTime": [
        {"bookName": "brave new world", "scores": [{"year": 1932, "score": 65}, {"year": 2000, "score": 80}, {"year": 2021, "score": 70}]},
        {"bookName": "the lord of the rings", "scores": [{"year": 1954, "score": 75}, {"year": 2001, "score": 95}, {"year": 2021, "score": 90}]},
        {"bookName": "three body problem", "scores": [{"year": 2008, "score": 60}, {"year": 2014, "score": 79}, {"year": 2021, "score": 85}]},
        {"bookName": "stormlight archive", "scores": [{"year": 2010, "score": 76}, {"year": 2020, "score": 85}, {"year": 2021, "score": 81}]}
      ]
    });
    assert_eq!(json_value, expected);
}

test_deserializer!(
    test_object_from_string_ordered_class,
    r#"
  {
    "one": "one",
    "two": "two",
    "three": "three",
    "four": "four"
  }
  "#,
    class_ty("OrderedClass", vec![
        optional_field("one", string_ty()),
        field("two", string_ty()),
        optional_field("three", string_ty()),
        field("four", string_ty()),
    ]),
    empty_db(),
    {
      "one": "one",
      "two": "two",
      "three": "three",
      "four": "four"
    }
);

test_deserializer!(
    test_leading_close_braces,
    r#"]
  {
    "one": "one",
    "two": "two",
    "three": "three",
    "four": "four"
  }
    "#,
    class_ty("OrderedClass", vec![
        optional_field("one", string_ty()),
        field("two", string_ty()),
        optional_field("three", string_ty()),
        field("four", string_ty()),
    ]),
    empty_db(),
    {
      "one": "one",
      "two": "two",
      "three": "three",
      "four": "four"
    }
);
