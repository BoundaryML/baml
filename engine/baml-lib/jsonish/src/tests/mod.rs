#[macro_use]
pub mod macros;

mod test_class;
mod test_enum;
mod test_lists;

use std::path::PathBuf;

use baml_types::BamlValue;
use internal_baml_core::{
    internal_baml_diagnostics::SourceFile,
    ir::{repr::IntermediateRepr, FieldType, TypeValue},
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
    schema.diagnostics.to_result().unwrap();

    IntermediateRepr::from_parser_database(&schema.db, schema.configuration).unwrap()
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
  id string
}
"#;
test_deserializer!(
    test_string_from_string23,
    FOO_FILE,
    r#"Hello there. Here is {playername}

    JSON Output:
    ```json
      {
        "id": "{{hi} there"
      }
    ```
    "#,
    FieldType::Class("Foo".to_string()),
    json!({"id": r#"{{hi} there"# })
);

// This fails
// test_deserializer!(
//     test_string_from_string23,
//     FOO_FILE,
//     r#"Hello there. Here is {{playername}

//   JSON Output:

//     {
//       "id": "{{hi} there"
//     }

//   "#,
//     FieldType::Class("Foo".to_string()),
//     json!({"id": r#"{{hi} there"# })
// );

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
    FieldType::Class("Foo".to_string()),
    json!({"id": r#"{{hi} there"# })
);
