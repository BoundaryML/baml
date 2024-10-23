use super::*;

const BAML_FILE: &str = r###"
class Score {
  year int @description(#"
    The year you're giving the score for.
  "#)
  score int @description(#"
    1 to 100
  "#)
}

class PopularityOverTime {
  bookName string
  scores Score[]
}

class WordCount {
  bookName string
  count int
}

class Ranking {
  bookName string
  score int @description(#"
    1 to 100 of your own personal score of this book
  "#)
}
 
class BookAnalysis {
  bookNames string[] @description(#"
    The list of book names  provided
  "#)
  popularityOverTime PopularityOverTime[] @description(#"
    Print the popularity of EACH BOOK over time.
    Make sure you add datapoints up to the current year. Try to use a max of 10 datapoints to 
    represent the whole timeline for all books (so 10 handpicked years).
  "#) @alias(popularityData)
  popularityRankings Ranking[] @description(#"
    A list of the book's popularity rankings over time. 
    The first element is the top ranking.
  "#)
  wordCounts WordCount[]
} 
"###;

test_partial_deserializer!(
    test_partial_analysis_1,
    BAML_FILE,
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
    FieldType::Class("BookAnalysis".to_string()),
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
        {"bookName": "the lord of the rings", "count": 470000},
        {"bookName": "three body problem", "count": 150000},
        {"bookName": "stormlight archive", "count": 400000}
      ]
    }
);

test_partial_deserializer!(
  test_partial_analysis_2,
  BAML_FILE,
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
  FieldType::Class("BookAnalysis".to_string()),
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

const CHOPPY_BAML_FILE: &str = r##"
class Error {
  code int
  message string
}

// Technically, everything can cast to this object.
class ErrorBasic {
  message string
}

class GraphJson {
  vertices Vertex[]
  edges Edge[]
}

class Vertex {
  id string @description(#"
    A unique human-readable identifier for the vertex, like 'peter'
  "#)
  metadata map<string, string> @description(#"
    Arbitrary metadata for the vertex, like 'name' or 'age'
  "#)
}

class Edge {
  source_id string
  target_id string
  // note, you could use an enum here if you know what the relationships are
  relationship string @description(#"
    A human-readable label for the edge, like 'knows' or "works_with", etc..
  "#)
}
  "##;

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

test_partial_deserializer!(
  test_partial_choppy,
  CHOPPY_BAML_FILE,
  TRIMMED_CHOPPY_RESULT,
  FieldType::Class("GraphJson".to_string()),
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

test_partial_deserializer!(
  test_partial_choppy_union,
  CHOPPY_BAML_FILE,
  TRIMMED_CHOPPY_RESULT,
  FieldType::union(vec![FieldType::Class("GraphJson".to_string()), FieldType::Class("GraphJson".to_string()).as_list(), FieldType::Class("Error".to_string())]),
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

test_partial_deserializer!(
  test_partial_choppy_union_2,
  CHOPPY_BAML_FILE,
  TRIMMED_CHOPPY_RESULT,
  FieldType::union(vec![FieldType::Class("GraphJson".to_string()), FieldType::Class("ErrorBasic".to_string())]),
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
