use super::*;

/// Helper: build all the types for the BookAnalysis schema and return
/// (book_analysis_ty, db) ready for use in tests.
fn book_analysis_schema() -> (
    TyResolved<'static, &'static str>,
    TypeRefDb<'static, &'static str>,
) {
    let score_cls = class_ty(
        "Score",
        vec![field("year", int_ty()), field("score", int_ty())],
    );
    let pop_cls = class_ty(
        "PopularityOverTime",
        vec![
            field("bookName", string_ty()),
            AnnotatedField {
                name: Cow::Borrowed("scores"),
                ty: annotated(array_of(annotated(Ty::Unresolved("Score")))),
                class_in_progress_field_missing: AttrLiteral::Array(vec![]),
                class_completed_field_missing: AttrLiteral::Never,
                aliases: vec![],
            },
        ],
    );
    let word_count_cls = class_ty(
        "WordCount",
        vec![field("bookName", string_ty()), field("count", int_ty())],
    );
    let ranking_cls = class_ty(
        "Ranking",
        vec![field("bookName", string_ty()), field("score", int_ty())],
    );
    let book_analysis = class_ty(
        "BookAnalysis",
        vec![
            AnnotatedField {
                name: Cow::Borrowed("bookNames"),
                ty: annotated(array_of(annotated(string_ty()))),
                class_in_progress_field_missing: AttrLiteral::Array(vec![]),
                class_completed_field_missing: AttrLiteral::Never,
                aliases: vec![],
            },
            AnnotatedField {
                name: Cow::Borrowed("popularityOverTime"),
                ty: annotated(array_of(annotated(Ty::Unresolved("PopularityOverTime")))),
                class_in_progress_field_missing: AttrLiteral::Array(vec![]),
                class_completed_field_missing: AttrLiteral::Never,
                aliases: vec![Cow::Borrowed("popularityData")],
            },
            AnnotatedField {
                name: Cow::Borrowed("popularityRankings"),
                ty: annotated(array_of(annotated(Ty::Unresolved("Ranking")))),
                class_in_progress_field_missing: AttrLiteral::Array(vec![]),
                class_completed_field_missing: AttrLiteral::Never,
                aliases: vec![],
            },
            AnnotatedField {
                name: Cow::Borrowed("wordCounts"),
                ty: annotated(array_of(annotated(Ty::Unresolved("WordCount")))),
                class_in_progress_field_missing: AttrLiteral::Array(vec![]),
                class_completed_field_missing: AttrLiteral::Never,
                aliases: vec![],
            },
        ],
    );

    let mut db = TypeRefDb::new();
    db.try_add("Score", score_cls).ok().unwrap();
    db.try_add("PopularityOverTime", pop_cls).ok().unwrap();
    db.try_add("WordCount", word_count_cls).ok().unwrap();
    db.try_add("Ranking", ranking_cls).ok().unwrap();

    (book_analysis, db)
}

/// Helper: build all the types for the choppy (GraphJson / Error) schema and
/// return (db) — callers pick which top-level type to target.
fn choppy_schema() -> TypeRefDb<'static, &'static str> {
    let error_cls = class_ty(
        "Error",
        vec![field("code", int_ty()), field("message", string_ty())],
    );
    let error_basic_cls = class_ty("ErrorBasic", vec![field("message", string_ty())]);
    let vertex_cls = class_ty(
        "Vertex",
        vec![
            field("id", string_ty()),
            AnnotatedField {
                name: Cow::Borrowed("metadata"),
                ty: annotated(map_of(annotated(string_ty()), annotated(string_ty()))),
                class_in_progress_field_missing: AttrLiteral::Map(IndexMap::new()),
                class_completed_field_missing: AttrLiteral::Never,
                aliases: vec![],
            },
        ],
    );
    let edge_cls = class_ty(
        "Edge",
        vec![
            field("source_id", string_ty()),
            field("target_id", string_ty()),
            field("relationship", string_ty()),
        ],
    );
    let graph_json_cls = class_ty(
        "GraphJson",
        vec![
            AnnotatedField {
                name: Cow::Borrowed("vertices"),
                ty: annotated(array_of(annotated(Ty::Unresolved("Vertex")))),
                class_in_progress_field_missing: AttrLiteral::Array(vec![]),
                class_completed_field_missing: AttrLiteral::Never,
                aliases: vec![],
            },
            AnnotatedField {
                name: Cow::Borrowed("edges"),
                ty: annotated(array_of(annotated(Ty::Unresolved("Edge")))),
                class_in_progress_field_missing: AttrLiteral::Array(vec![]),
                class_completed_field_missing: AttrLiteral::Never,
                aliases: vec![],
            },
        ],
    );

    let mut db = TypeRefDb::new();
    db.try_add("Error", error_cls).ok().unwrap();
    db.try_add("ErrorBasic", error_basic_cls).ok().unwrap();
    db.try_add("Vertex", vertex_cls).ok().unwrap();
    db.try_add("Edge", edge_cls).ok().unwrap();
    db.try_add("GraphJson", graph_json_cls).ok().unwrap();

    db
}

const TRIMMED_CHOPPY_RESULT: &str = r#"
```json
{
  "vertices": [
    {
      "id": "stephanie_morales",
      "metadata": {
        "name": "Stephanie Morales",
        "affiliation": "Made Space"
      }
    },
    {
      "id":
  "#;

// ---------------------------------------------------------------------------
// Test 1: Full BookAnalysis with all fields complete
// ---------------------------------------------------------------------------
#[test]
fn test_partial_analysis_1() {
    let (book_analysis, db) = book_analysis_schema();

    let raw = r#"
    ```json
    {
      "bookNames": [
        "brave new world",
        "the lord of the rings",
        "three body problem",
        "stormlight archive"
      ],
      "popularityData": [
        {
          "bookName": "brave new world",
          "scores": [
            {"year": 1950, "score": 70},
            {"year": 1960, "score": 75},
            {"year": 1970, "score": 80},
            {"year": 1980, "score": 85},
            {"year": 1990, "score": 85},
            {"year": 2000, "score": 90},
            {"year": 2010, "score": 95},
            {"year": 2020, "score": 97},
            {"year": 2023, "score": 98}
          ]
        },
        {
          "bookName": "the lord of the rings",
          "scores": [
            {"year": 1954, "score": 60},
            {"year": 1960, "score": 75},
            {"year": 1970, "score": 85},
            {"year": 1980, "score": 90},
            {"year": 1990, "score": 92},
            {"year": 2000, "score": 95},
            {"year": 2010, "score": 96},
            {"year": 2020, "score": 98},
            {"year": 2023, "score": 99}
          ]
        },
        {
          "bookName": "three body problem",
          "scores": [
            {"year": 2008, "score": 50},
            {"year": 2010, "score": 60},
            {"year": 2015, "score": 70},
            {"year": 2020, "score": 80},
            {"year": 2023, "score": 85}
          ]
        },
        {
          "bookName": "stormlight archive",
          "scores": [
            {"year": 2010, "score": 55},
            {"year": 2014, "score": 65},
            {"year": 2017, "score": 75},
            {"year": 2020, "score": 80},
            {"year": 2023, "score": 85}
          ]
        }
      ],
      "popularityRankings": [
        {"bookName": "the lord of the rings", "score": 99},
        {"bookName": "brave new world", "score": 97},
        {"bookName": "stormlight archive", "score": 85},
        {"bookName": "three body problem", "score": 85}
      ],
      "wordCounts": [
        {"bookName": "brave new world", "count": 64000},
        {"bookName": "the lord of the rings", "count": 470000},
        {"bookName": "three body problem", "count": 150000},
        {"bookName": "stormlight archive", "count": 400000}
      ]
    }
    ```
    "#;

    let parsed =
        crate::jsonish::parse(raw, Default::default(), false).expect("jsonish::parse failed");
    let ctx = crate::deserializer::coercer::ParsingContext::new(book_analysis.as_ref(), &db);
    let annots = TypeAnnotations::default();
    let target = TyWithMeta::new(book_analysis.as_ref(), &annots);
    let result = TyResolvedRef::coerce(&ctx, target, &parsed);
    assert!(result.is_ok(), "Failed to parse: {:?}", result);
    let value = result.unwrap();
    assert!(
        value.is_some(),
        "Coercion returned None (in_progress=never?)"
    );
    let value = value.unwrap();
    let json_value = serde_json::to_value(&value).unwrap();
    let expected = serde_json::json!({
      "bookNames": [
        "brave new world",
        "the lord of the rings",
        "three body problem",
        "stormlight archive"
      ],
      "popularityOverTime": [
        {
          "bookName": "brave new world",
          "scores": [
            {"year": 1950, "score": 70},
            {"year": 1960, "score": 75},
            {"year": 1970, "score": 80},
            {"year": 1980, "score": 85},
            {"year": 1990, "score": 85},
            {"year": 2000, "score": 90},
            {"year": 2010, "score": 95},
            {"year": 2020, "score": 97},
            {"year": 2023, "score": 98}
          ]
        },
        {
          "bookName": "the lord of the rings",
          "scores": [
            {"year": 1954, "score": 60},
            {"year": 1960, "score": 75},
            {"year": 1970, "score": 85},
            {"year": 1980, "score": 90},
            {"year": 1990, "score": 92},
            {"year": 2000, "score": 95},
            {"year": 2010, "score": 96},
            {"year": 2020, "score": 98},
            {"year": 2023, "score": 99}
          ]
        },
        {
          "bookName": "three body problem",
          "scores": [
            {"year": 2008, "score": 50},
            {"year": 2010, "score": 60},
            {"year": 2015, "score": 70},
            {"year": 2020, "score": 80},
            {"year": 2023, "score": 85}
          ]
        },
        {
          "bookName": "stormlight archive",
          "scores": [
            {"year": 2010, "score": 55},
            {"year": 2014, "score": 65},
            {"year": 2017, "score": 75},
            {"year": 2020, "score": 80},
            {"year": 2023, "score": 85}
          ]
        }
      ],
      "popularityRankings": [
        {"bookName": "the lord of the rings", "score": 99},
        {"bookName": "brave new world", "score": 97},
        {"bookName": "stormlight archive", "score": 85},
        {"bookName": "three body problem", "score": 85}
      ],
      "wordCounts": [
        {"bookName": "brave new world", "count": 64000},
        {"bookName": "the lord of the rings", "count": 470000},
        {"bookName": "three body problem", "count": 150000},
        {"bookName": "stormlight archive", "count": 400000}
      ]
    });
    assert_eq!(json_value, expected);
}

// ---------------------------------------------------------------------------
// Test 2: Partial BookAnalysis with data cut off mid-stream
// ---------------------------------------------------------------------------
#[test]
fn test_partial_analysis_2() {
    let (book_analysis, db) = book_analysis_schema();

    let raw = r#"
  ```json
  {
    "bookNames": [
      "brave new world",
      "the lord of the rings",
      "three body problem",
      "stormlight archive"
    ],
    "popularityData": [
      {
        "bookName": "brave new world",
        "scores": [
          {"year": 1950, "score": 70},
  "#;

    let parsed =
        crate::jsonish::parse(raw, Default::default(), false).expect("jsonish::parse failed");
    let ctx = crate::deserializer::coercer::ParsingContext::new(book_analysis.as_ref(), &db);
    let annots = TypeAnnotations::default();
    let target = TyWithMeta::new(book_analysis.as_ref(), &annots);
    let result = TyResolvedRef::coerce(&ctx, target, &parsed);
    assert!(result.is_ok(), "Failed to parse: {:?}", result);
    let value = result.unwrap();
    assert!(
        value.is_some(),
        "Coercion returned None (in_progress=never?)"
    );
    let value = value.unwrap();
    let json_value = serde_json::to_value(&value).unwrap();
    let expected = serde_json::json!({
      "bookNames": [
        "brave new world",
        "the lord of the rings",
        "three body problem",
        "stormlight archive"
      ],
      "popularityOverTime": [
        {
          "bookName": "brave new world",
          "scores": [
            {"year": 1950, "score": 70}
          ]
        }
      ],
      "popularityRankings": [],
      "wordCounts": []
    });
    assert_eq!(json_value, expected);
}

// ---------------------------------------------------------------------------
// Test 3: Partial GraphJson with incomplete vertex
// ---------------------------------------------------------------------------
test_partial_deserializer!(
    test_partial_choppy,
    TRIMMED_CHOPPY_RESULT,
    class_ty("GraphJson", vec![
        AnnotatedField {
            name: Cow::Borrowed("vertices"),
            ty: annotated(array_of(annotated(Ty::Unresolved("Vertex")))),
            class_in_progress_field_missing: AttrLiteral::Array(vec![]),
            class_completed_field_missing: AttrLiteral::Never,
            aliases: vec![],
        },
        AnnotatedField {
            name: Cow::Borrowed("edges"),
            ty: annotated(array_of(annotated(Ty::Unresolved("Edge")))),
            class_in_progress_field_missing: AttrLiteral::Array(vec![]),
            class_completed_field_missing: AttrLiteral::Never,
            aliases: vec![],
        },
    ]),
    choppy_schema(),
    {
      "vertices": [
        {
          "id": "stephanie_morales",
          "metadata": {
            "name": "Stephanie Morales",
            "affiliation": "Made Space"
          }
        },
        {
          "id": null,
          "metadata": {
          }
        }
      ],
      "edges": [
      ]
    }
);

// ---------------------------------------------------------------------------
// Test 4: Union of [GraphJson, GraphJson[], Error] with incomplete vertex
// ---------------------------------------------------------------------------
test_partial_deserializer!(
    test_partial_choppy_union,
    TRIMMED_CHOPPY_RESULT,
    union_of(vec![
        annotated(Ty::Unresolved("GraphJson")),
        annotated(array_of(annotated(Ty::Unresolved("GraphJson")))),
        annotated(Ty::Unresolved("Error")),
    ]),
    choppy_schema(),
    {
      "vertices": [
        {
          "id": "stephanie_morales",
          "metadata": {
            "name": "Stephanie Morales",
            "affiliation": "Made Space"
          }
        },
        {
          "id": null,
          "metadata": {
          }
        }
      ],
      "edges": [
      ]
    }
);

// ---------------------------------------------------------------------------
// Test 5: Union of [GraphJson, ErrorBasic] with incomplete vertex
// ---------------------------------------------------------------------------
test_partial_deserializer!(
    test_partial_choppy_union_2,
    TRIMMED_CHOPPY_RESULT,
    union_of(vec![
        annotated(Ty::Unresolved("GraphJson")),
        annotated(Ty::Unresolved("ErrorBasic")),
    ]),
    choppy_schema(),
    {
      "vertices": [
        {
          "id": "stephanie_morales",
          "metadata": {
            "name": "Stephanie Morales",
            "affiliation": "Made Space"
          }
        },
        {
          "id": null,
          "metadata": {
          }
        }
      ],
      "edges": [
      ]
    }
);
