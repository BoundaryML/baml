#[macro_use]
mod macros;

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
use crate::{baml_db, baml_tyannotated};

// ---------------------------------------------------------------------------
// Inline tests from old mod.rs (primitives + simple classes)
// ---------------------------------------------------------------------------

test_deserializer!(
    test_string_from_string,
    r#"hello"#,
    baml_tyannotated!(string),
    baml_db! {},
    "hello"
);

test_deserializer!(
    test_string_from_string_with_quotes,
    r#""hello""#,
    baml_tyannotated!(string),
    baml_db! {},
    "\"hello\""
);

test_deserializer!(
    test_string_from_object,
    r#"{"hi":    "hello"}"#,
    baml_tyannotated!(string),
    baml_db! {},
    r#"{"hi":    "hello"}"#
);

test_deserializer!(
    test_string_from_obj_and_string,
    r#"The output is: {"hello": "world"}"#,
    baml_tyannotated!(string),
    baml_db! {},
    "The output is: {\"hello\": \"world\"}"
);

test_deserializer!(
    test_string_from_list,
    r#"["hello", "world"]"#,
    baml_tyannotated!(string),
    baml_db! {},
    "[\"hello\", \"world\"]"
);

test_deserializer!(
    test_string_from_int,
    r#"1"#,
    baml_tyannotated!(string),
    baml_db! {},
    "1"
);

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
    baml_tyannotated!(string),
    baml_db! {},
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
    baml_tyannotated!(string),
    baml_db! {},
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
    baml_tyannotated!(Foo),
    baml_db!{
        class Foo {
            id: (string | null) @class_in_progress_field_missing(null) @class_completed_field_missing(null),
        }
    },
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
    baml_tyannotated!(Foo),
    baml_db!{
        class Foo {
            id: (string | null) @class_completed_field_missing(null),
        }
    },
    {"id": r#"{{hi} there"# }
);

// BookAnalysis example
test_deserializer!(
    test_string_from_string25,
    r#"
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
    "#,
    baml_tyannotated!(BookAnalysis),
    baml_db!{
        class Score {
            year: int,
            score: int,
        }
        class PopularityOverTime {
            bookName: string,
            scores: [Score],
        }
        class BookAnalysis {
            bookNames: [string],
            popularityOverTime: [PopularityOverTime] @alias("popularityData"),
        }
    },
    {
      "bookNames": ["brave new world", "the lord of the rings", "three body problem", "stormlight archive"],
      "popularityOverTime": [
        {"bookName": "brave new world", "scores": [{"year": 1932, "score": 65}, {"year": 2000, "score": 80}, {"year": 2021, "score": 70}]},
        {"bookName": "the lord of the rings", "scores": [{"year": 1954, "score": 75}, {"year": 2001, "score": 95}, {"year": 2021, "score": 90}]},
        {"bookName": "three body problem", "scores": [{"year": 2008, "score": 60}, {"year": 2014, "score": 79}, {"year": 2021, "score": 85}]},
        {"bookName": "stormlight archive", "scores": [{"year": 2010, "score": 76}, {"year": 2020, "score": 85}, {"year": 2021, "score": 81}]}
      ]
    }
);

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
    baml_tyannotated!(OrderedClass),
    baml_db!{
        class OrderedClass {
            one: (string | null) @class_completed_field_missing(null),
            two: string,
            three: (string | null) @class_completed_field_missing(null),
            four: string,
        }
    },
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
    baml_tyannotated!(OrderedClass),
    baml_db!{
        class OrderedClass {
            one: (string | null) @class_completed_field_missing(null),
            two: string,
            three: (string | null) @class_completed_field_missing(null),
            four: string,
        }
    },
    {
      "one": "one",
      "two": "two",
      "three": "three",
      "four": "four"
    }
);
