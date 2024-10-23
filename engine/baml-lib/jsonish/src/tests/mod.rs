use anyhow::Result;
use internal_baml_jinja::types::{Class, Enum, Name, OutputFormatContent};

#[macro_use]
pub mod macros;

mod test_basics;
mod test_class;
mod test_enum;
mod test_lists;
mod test_literals;
mod test_maps;
mod test_partials;
mod test_unions;

use std::{
    collections::{HashMap, HashSet},
    path::PathBuf,
};

use baml_types::BamlValue;
use internal_baml_core::{
    internal_baml_diagnostics::SourceFile,
    ir::{repr::IntermediateRepr, ClassWalker, EnumWalker, FieldType, IRHelper, TypeValue},
    validate,
};
use serde_json::json;

use crate::from_str;

fn load_test_ir(file_content: &str) -> IntermediateRepr {
    let mut schema = validate(
        &PathBuf::from("./baml_src"),
        vec![SourceFile::from((
            PathBuf::from("./baml_src/example.baml"),
            file_content.to_string(),
        ))],
    );
    match schema.diagnostics.to_result() {
        Ok(_) => {}
        Err(e) => {
            panic!("Failed to validate schema: {}", e);
        }
    }

    IntermediateRepr::from_parser_database(&schema.db, schema.configuration).unwrap()
}

fn render_output_format(
    ir: &IntermediateRepr,
    output: &FieldType,
    env_values: &HashMap<String, String>,
) -> Result<OutputFormatContent> {
    let (enums, classes) = relevant_data_models(ir, output, env_values)?;
    return Ok(OutputFormatContent::new(enums, classes, output.clone()));
}

fn find_existing_class_field<'a>(
    class_name: &str,
    field_name: &str,
    class_walker: &Result<ClassWalker<'a>>,
    env_values: &HashMap<String, String>,
) -> Result<(Name, FieldType, Option<String>)> {
    let Ok(class_walker) = class_walker else {
        anyhow::bail!("Class {} does not exist", class_name);
    };

    let Some(field_walker) = class_walker.find_field(field_name) else {
        anyhow::bail!("Class {} does not have a field: {}", class_name, field_name);
    };

    let name = Name::new_with_alias(field_name.to_string(), field_walker.alias(env_values)?);
    let desc = field_walker.description(env_values)?;
    let r#type = field_walker.r#type();
    Ok((name, r#type.clone(), desc))
}

fn find_enum_value(
    enum_name: &str,
    value_name: &str,
    enum_walker: &Result<EnumWalker<'_>>,
    env_values: &HashMap<String, String>,
) -> Result<Option<(Name, Option<String>)>> {
    if enum_walker.is_err() {
        anyhow::bail!("Enum {} does not exist", enum_name);
    }

    let value_walker = match enum_walker {
        Ok(e) => e.find_value(value_name),
        Err(_) => None,
    };

    let value_walker = match value_walker {
        Some(v) => v,
        None => return Ok(None),
    };

    if value_walker.skip(env_values)? {
        return Ok(None);
    }

    let name = Name::new_with_alias(value_name.to_string(), value_walker.alias(env_values)?);
    let desc = value_walker.description(env_values)?;

    Ok(Some((name, desc)))
}

fn relevant_data_models<'a>(
    ir: &'a IntermediateRepr,
    output: &'a FieldType,
    env_values: &HashMap<String, String>,
) -> Result<(Vec<Enum>, Vec<Class>)> {
    let mut checked_types = HashSet::new();
    let mut enums = Vec::new();
    let mut classes = Vec::new();
    let mut start: Vec<baml_types::FieldType> = vec![output.clone()];

    while !start.is_empty() {
        let output = start.pop().unwrap();
        match &output {
            FieldType::Enum(enm) => {
                if checked_types.insert(output.to_string()) {
                    let walker = ir.find_enum(enm);

                    let real_values = walker
                        .as_ref()
                        .map(|e| e.walk_values().map(|v| v.name().to_string()))
                        .ok();
                    let values = real_values
                        .into_iter()
                        .flatten()
                        .into_iter()
                        .map(|value| {
                            let meta = find_enum_value(enm, &value, &walker, env_values)?;
                            Ok(meta.map(|m| m))
                        })
                        .filter_map(|v| v.transpose())
                        .collect::<Result<Vec<_>>>()?;

                    enums.push(Enum {
                        name: Name::new_with_alias(enm.to_string(), walker?.alias(env_values)?),
                        values,
                    });
                }
            }
            FieldType::List(inner) | FieldType::Optional(inner) => {
                if !checked_types.contains(&inner.to_string()) {
                    start.push(inner.as_ref().clone());
                }
            }
            FieldType::Map(k, v) => {
                if checked_types.insert(output.to_string()) {
                    if !checked_types.contains(&k.to_string()) {
                        start.push(k.as_ref().clone());
                    }
                    if !checked_types.contains(&v.to_string()) {
                        start.push(v.as_ref().clone());
                    }
                }
            }
            FieldType::Tuple(options) | FieldType::Union(options) => {
                if checked_types.insert((&output).to_string()) {
                    for inner in options {
                        if !checked_types.contains(&inner.to_string()) {
                            start.push(inner.clone());
                        }
                    }
                }
            }
            FieldType::Class(cls) => {
                if checked_types.insert(output.to_string()) {
                    let walker = ir.find_class(&cls);

                    let real_fields = walker
                        .as_ref()
                        .map(|e| e.walk_fields().map(|v| v.name().to_string()))
                        .ok();

                    let fields = real_fields.into_iter().flatten().into_iter().map(|field| {
                        let meta = find_existing_class_field(&cls, &field, &walker, env_values)?;
                        Ok(meta)
                    });

                    let fields = fields.collect::<Result<Vec<_>>>()?;

                    for (_, t, _) in fields.iter().as_ref() {
                        if !checked_types.contains(&t.to_string()) {
                            start.push(t.clone());
                        }
                    }

                    classes.push(Class {
                        name: Name::new_with_alias(cls.to_string(), walker?.alias(env_values)?),
                        fields,
                    });
                }
            }
            FieldType::Primitive(_) => {}
            FieldType::Literal(_) => {}
        }
    }

    Ok((enums, classes))
}

const EMPTY_FILE: &str = r#"
"#;

test_deserializer!(
    test_string_from_string,
    EMPTY_FILE,
    r#"hello"#,
    FieldType::Primitive(TypeValue::String),
    "hello"
);

test_deserializer!(
    test_string_from_string_with_quotes,
    EMPTY_FILE,
    r#""hello""#,
    FieldType::Primitive(TypeValue::String),
    "\"hello\""
);

test_deserializer!(
    test_string_from_object,
    EMPTY_FILE,
    r#"{"hi":    "hello"}"#,
    FieldType::Primitive(TypeValue::String),
    r#"{"hi":    "hello"}"#
);

test_deserializer!(
    test_string_from_obj_and_string,
    EMPTY_FILE,
    r#"The output is: {"hello": "world"}"#,
    FieldType::Primitive(TypeValue::String),
    "The output is: {\"hello\": \"world\"}"
);

test_deserializer!(
    test_string_from_list,
    EMPTY_FILE,
    r#"["hello", "world"]"#,
    FieldType::Primitive(TypeValue::String),
    "[\"hello\", \"world\"]"
);

test_deserializer!(
    test_string_from_int,
    EMPTY_FILE,
    r#"1"#,
    FieldType::Primitive(TypeValue::String),
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
    FieldType::Primitive(TypeValue::String),
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
    FieldType::Primitive(TypeValue::String),
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
    FieldType::Class("Foo".to_string()),
    json!({"id": null })
);

// also fails -- if you are in an object and you are casting to a string, don't do that.
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
    FieldType::Class("Foo".to_string()),
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
    FieldType::Class("BookAnalysis".to_string()),
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
    FieldType::Class("BookAnalysis".to_string()),
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
    FieldType::Class("OrderedClass".to_string()),
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
    FieldType::Class("OrderedClass".to_string()),
    json!({
      "one": "one",
      "two": "two",
      "three": "three",
      "four": "four"
    })
);

#[test]
/// Test that when partial parsing, if we encounter an int in a context
/// where it could possibly be extended further, it must be returned
/// as Null.
fn singleton_list_int_deleted() {
    let target = FieldType::List(Box::new(FieldType::Primitive(TypeValue::Int)));
    let output_format = OutputFormatContent::new(Vec::new(), Vec::new(), target.clone());
    let res = from_str(&output_format, &target, "[123", true).expect("Can parse");
    let baml_value: BamlValue = res.into();
    assert_eq!(baml_value, BamlValue::List(vec![]));
}

#[test]
/// Test that when partial parsing, if we encounter an int in a context
/// where it could possibly be extended further, it must be returned
/// as Null.
fn list_int_deleted() {
    let target = FieldType::List(Box::new(FieldType::Primitive(TypeValue::Int)));
    let output_format = OutputFormatContent::new(Vec::new(), Vec::new(), target.clone());
    let res = from_str(&output_format, &target, "[123, 456", true).expect("Can parse");
    let baml_value: BamlValue = res.into();
    assert_eq!(baml_value, BamlValue::List(vec![BamlValue::Int(123)]));
}

#[test]
/// Test that when partial parsing, if we encounter an int in a context
/// where it could possibly be extended further, it must be returned
/// as Null.
fn list_int_not_deleted() {
    let target = FieldType::List(Box::new(FieldType::Primitive(TypeValue::Int)));
    let output_format = OutputFormatContent::new(Vec::new(), Vec::new(), target.clone());
    let res = from_str(&output_format, &target, "[123, 456 // Done", true).expect("Can parse");
    let baml_value: BamlValue = res.into();
    assert_eq!(
      baml_value,
      BamlValue::List( vec![ BamlValue::Int(123), BamlValue::Int(456)])
    );
}

#[test]
/// Test that when partial parsing, if we encounter an int in a context
/// where it could possibly be extended further, it must be returned
/// as Null.
fn partial_int_deleted() {
    let target = FieldType::Optional(Box::new(FieldType::Primitive(TypeValue::Int)));
    let output_format = OutputFormatContent::new(Vec::new(), Vec::new(), target.clone());
    let res = from_str(&output_format, &target, "123", true).expect("Can parse");
    let baml_value: BamlValue = res.into();
    // Note: This happens to parse as a List, but Null also seems appropriate.
    assert_eq!(baml_value, BamlValue::Null);
}

#[test]
/// Test that when partial parsing, if we encounter an int in a context
/// where it could possibly be extended further, it must be returned
/// as Null.
fn partial_int_not_deleted() {
    let target = FieldType::List(Box::new(FieldType::Primitive(TypeValue::Int)));
    let output_format = OutputFormatContent::new(Vec::new(), Vec::new(), target.clone());
    let res = from_str(&output_format, &target, "123", true).expect("Can parse");
    let baml_value: BamlValue = res.into();
    // Note: This happens to parse as a List, but Null also seems appropriate.
    assert_eq!(baml_value, BamlValue::List(vec![]));
}
