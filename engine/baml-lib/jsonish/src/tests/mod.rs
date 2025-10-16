#[macro_use]
pub mod macros;

mod animation;
mod test_aliases;
mod test_basics;
mod test_class;
mod test_class_2;
mod test_code;
mod test_constraints;
mod test_enum;
mod test_international;
mod test_lists;
mod test_literals;
mod test_maps;
mod test_partials;
mod test_streaming;
mod test_unions;

use std::{
    collections::{HashMap, HashSet},
    path::PathBuf,
};

use anyhow::Result;
use baml_types::{
    BamlValue, BamlValueWithMeta, CompletionState, EvaluationContext, JinjaExpression,
    ResponseCheck, TypeIR,
};
use indexmap::{IndexMap, IndexSet};
use internal_baml_core::{
    ast::Field,
    internal_baml_diagnostics::SourceFile,
    ir::{repr::IntermediateRepr, ClassWalker, EnumWalker, IRHelper, TypeValue},
    validate,
};
use internal_baml_jinja::types::{Class, Enum, Name, OutputFormatContent};
use serde_json::json;

use crate::{
    deserializer::{deserialize_flags::Flag, semantic_streaming::validate_streaming_state},
    from_str, BamlValueWithFlags, ResponseBamlValue,
};

const EMPTY_FILE: &str = r#"
"#;

test_deserializer!(
    test_string_from_string,
    EMPTY_FILE,
    r#"hello"#,
    TypeIR::Primitive(TypeValue::String, Default::default()),
    "hello"
);

test_deserializer!(
    test_string_from_string_with_quotes,
    EMPTY_FILE,
    r#""hello""#,
    TypeIR::Primitive(TypeValue::String, Default::default()),
    "\"hello\""
);

test_deserializer!(
    test_string_from_object,
    EMPTY_FILE,
    r#"{"hi":    "hello"}"#,
    TypeIR::Primitive(TypeValue::String, Default::default()),
    r#"{"hi":    "hello"}"#
);

test_deserializer!(
    test_string_from_obj_and_string,
    EMPTY_FILE,
    r#"The output is: {"hello": "world"}"#,
    TypeIR::Primitive(TypeValue::String, Default::default()),
    "The output is: {\"hello\": \"world\"}"
);

test_deserializer!(
    test_string_from_list,
    EMPTY_FILE,
    r#"["hello", "world"]"#,
    TypeIR::Primitive(TypeValue::String, Default::default()),
    "[\"hello\", \"world\"]"
);

test_deserializer!(
    test_string_from_int,
    EMPTY_FILE,
    r#"1"#,
    TypeIR::Primitive(TypeValue::String, Default::default()),
    "1"
);

test_deserializer!(
    test_string_from_string21,
    EMPTY_FILE,
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
    TypeIR::Primitive(TypeValue::String, Default::default()),
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
    EMPTY_FILE,
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
    TypeIR::Primitive(TypeValue::String, Default::default()),
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

const FOO_FILE: &str = r#"
class Foo {
  id string?
}
"#;

// This fails because we cannot find the inner json blob
test_deserializer!(
    test_string_from_string23,
    FOO_FILE,
    r#"Hello there. Here is {{playername}

  JSON Output:

    {
      "id": "{{hi} there"
    }

  "#,
    TypeIR::class("Foo"),
    json!({"id": null })
);

// also fails -- if you are in an object and you are casting to a string, dont do that.
// TODO: find all the json blobs here correctly
test_deserializer!(
    test_string_from_string24,
    FOO_FILE,
    r#"Hello there. Here is {playername}

    JSON Output:

      {
        "id": "{{hi} there",
      }

    "#,
    TypeIR::class("Foo"),
    json!({"id": r#"{{hi} there"# })
);

const EXAMPLE_FILE: &str = r##"
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
    "#) @alias(popularityData)
    // popularityRankings Ranking[] @description(#"
    //   A list of the book's popularity rankings over time. 
    //   The first element is the top ranking
    // "#)
   
    // wordCounts WordCount[]
  }
"##;

test_deserializer!(
    test_string_from_string25,
    EXAMPLE_FILE,
    r#"
    {
        "bookNames": ["brave new world", "the lord of the rings", "three body problem", "stormlight archive"],
        "popularityData": [
          {
            "bookName": "brave new world",
            "scores": [
              {
                "year": 1932,
                "score": 65
              },
              {
                "year": 2000,
                "score": 80
              },
              {
                "year": 2021,
                "score": 70
              }
            ]
          },
          {
            "bookName": "the lord of the rings",
            "scores": [
              {
                "year": 1954,
                "score": 75
              },
              {
                "year": 2001,
                "score": 95
              },
              {
                "year": 2021,
                "score": 90
              }
            ]
          },
          {
            "bookName": "three body problem",
            "scores": [
              {
                "year": 2008,
                "score": 60
              },
              {
                "year": 2014,
                "score": 79
              },
              {
                "year": 2021,
                "score": 85
              }
            ]
          },
          {
            "bookName": "stormlight archive",
            "scores": [
              {
                "year": 2010,
                "score": 76
              },
              {
                "year": 2020,
                "score": 85
              },
              {
                "year": 2021,
                "score": 81
              }
            ]
          }
        ]
      }
    "#,
    TypeIR::class("BookAnalysis"),
    json!({
      "bookNames": ["brave new world", "the lord of the rings", "three body problem", "stormlight archive"],
      "popularityOverTime": [
        {
          "bookName": "brave new world",
          "scores": [
            {
              "year": 1932,
              "score": 65
            },
            {
              "year": 2000,
              "score": 80
            },
            {
              "year": 2021,
              "score": 70
            }
          ]
        },
        {
          "bookName": "the lord of the rings",
          "scores": [
            {
              "year": 1954,
              "score": 75
            },
            {
              "year": 2001,
              "score": 95
            },
            {
              "year": 2021,
              "score": 90
            }
          ]
        },
        {
          "bookName": "three body problem",
          "scores": [
            {
              "year": 2008,
              "score": 60
            },
            {
              "year": 2014,
              "score": 79
            },
            {
              "year": 2021,
              "score": 85
            }
          ]
        },
        {
          "bookName": "stormlight archive",
          "scores": [
            {
              "year": 2010,
              "score": 76
            },
            {
              "year": 2020,
              "score": 85
            },
            {
              "year": 2021,
              "score": 81
            }
          ]
        }
      ]
    })
);

test_deserializer!(
    test_string_from_string26,
    EXAMPLE_FILE,
    r#"
  {
      "bookNames": ["brave new world", "the lord of the rings"],
      "popularityData": [
        {
          "bookName": "brave new world",
          "scores": [
            {
              "year": 1932,
              "score": 65
            }
          ]
        },
        {
          "bookName": "the lord of the rings",
          "scores": [
            {
              "year": 1954,
              "score": 75
            }
          ]
        },
        {
          "bookName": "the lord of the rings",
          "scores": [
            {
              "year": 1954,
              "score": 75
            }
          ]
        }
      ]
    }
  "#,
    TypeIR::class("BookAnalysis"),
    json!({
      "bookNames": ["brave new world", "the lord of the rings"],
      "popularityOverTime": [
        {
          "bookName": "brave new world",
          "scores": [
            {
              "year": 1932,
              "score": 65
            }
          ]
        },
        {
          "bookName": "the lord of the rings",
          "scores": [
            {
              "year": 1954,
              "score": 75
            }
          ]
        },
        {
          "bookName": "the lord of the rings",
          "scores": [
            {
              "year": 1954,
              "score": 75
            }
          ]
        }
      ]
    })
);

const EXAMPLE_FILE_ORDERED_CLASS: &str = r##"
  class OrderedClass {
    one string?
    two string
    three string?
    four string
  }
"##;

test_deserializer!(
    test_object_from_string_ordered_class,
    EXAMPLE_FILE_ORDERED_CLASS,
    r#"
  {
    "one": "one",
    "two": "two",
    "three": "three",
    "four": "four"
  }
  "#,
    TypeIR::class("OrderedClass"),
    json!({
      "one": "one",
      "two": "two",
      "three": "three",
      "four": "four"
    })
);

test_deserializer!(
    test_leading_close_braces,
    EXAMPLE_FILE_ORDERED_CLASS,
    r#"]
  {
    "one": "one",
    "two": "two",
    "three": "three",
    "four": "four"
  }
    "#,
    TypeIR::class("OrderedClass"),
    json!({
      "one": "one",
      "two": "two",
      "three": "three",
      "four": "four"
    })
);
