use super::*;
use crate::{baml_db, baml_ty};

/// Helper: build all the types for the `BookAnalysis` schema.
fn book_analysis_db() -> TypeRefDb<'static, &'static str> {
    baml_db! {
        class Score {
            year: int,
            score: int,
        }
        class PopularityOverTime {
            bookName: string,
            scores: [Score],
        }
        class WordCount {
            bookName: string,
            count: int,
        }
        class Ranking {
            bookName: string,
            score: int,
        }
        class BookAnalysis {
            bookNames: [string],
            popularityOverTime: [PopularityOverTime] @alias("popularityData"),
            popularityRankings: [Ranking],
            wordCounts: [WordCount],
        }
    }
}

/// Helper: build all the types for the choppy (`GraphJson` / Error) schema.
fn choppy_db() -> TypeRefDb<'static, &'static str> {
    baml_db! {
        class Error {
            code: int,
            message: string,
        }
        class ErrorBasic {
            message: string,
        }
        class Vertex {
            id: string,
            metadata: map<string, string>,
        }
        class Edge {
            source_id: string,
            target_id: string,
            relationship: string,
        }
        class GraphJson {
            vertices: [Vertex],
            edges: [Edge],
        }
    }
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
test_partial_deserializer!(
    test_partial_analysis_1,
    r#"
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
    "#,
    baml_ty!(BookAnalysis),
    book_analysis_db(),
    {
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
        {"bookName": "the lord of the rings", "count": 470_000},
        {"bookName": "three body problem", "count": 150_000},
        {"bookName": "stormlight archive", "count": 400_000}
      ]
    }
);

// ---------------------------------------------------------------------------
// Test 2: Partial BookAnalysis with data cut off mid-stream
// ---------------------------------------------------------------------------
test_partial_deserializer!(
    test_partial_analysis_2,
    r#"
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
  "#,
    baml_ty!(BookAnalysis),
    book_analysis_db(),
    {
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
    }
);

// ---------------------------------------------------------------------------
// Test 3: Partial GraphJson with incomplete vertex
// ---------------------------------------------------------------------------
test_partial_deserializer!(
    test_partial_choppy,
    TRIMMED_CHOPPY_RESULT,
    baml_ty!(GraphJson),
    choppy_db(),
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
          "id": "",
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
    baml_ty!((GraphJson | [GraphJson] | Error)),
    choppy_db(),
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
          "id": "",
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
    baml_ty!((GraphJson | ErrorBasic)),
    choppy_db(),
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
          "id": "",
          "metadata": {
          }
        }
      ],
      "edges": [
      ]
    }
);

// ---------------------------------------------------------------------------
// Test 6: \uXXXX escape inside a still-open string must decode
// (regression: the 4th hex digit used to be dropped, yielding the raw
// text "\u270 still streaming" instead of "✅ still streaming")
// ---------------------------------------------------------------------------
test_partial_deserializer!(
    test_partial_string_unicode_escape,
    r#"{"answer": "\u2705 still streaming"#,
    baml_tyannotated!(Answer),
    baml_db! {
        class Answer {
            answer: string @class_in_progress_field_missing(null),
        }
    },
    {"answer": "✅ still streaming"}
);
