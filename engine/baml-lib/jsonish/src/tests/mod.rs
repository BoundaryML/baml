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
    r#"When translating the game strings from English to Portuguese and Japanese, the first concern is to preserve the game world atmospheric impression and the sense of adventure. 

    For ID: CH1_Welcome, the objective is to welcome the player to the game "Arcadian Atlas". The translation should be straightforward without changing the game's title.
    
    For ID: CH1_02, the string introduces the player to the game world's challenges, emphasizing its dangers with the presence of monsters. The tone is designed to evoke excitement and a sense of adventure, which should also be maintained in the translated versions.
    
    For ID: CH1_03, there's a mission to save "Arcadia". The string also introduces a crucial character, "Jonathan", who seems to be the key to salvation. The Portuguese and Japanese versions need to recreate the sense of urgency and responsibility subtly presented in this string.
    
    JSON Output:
    ```json
    [
      {
        "id": "CH1_Welcome",
        "English": "Welcome to Arcadian Atlas",
        "Portuguese": "Bem-vindo ao Arcadian Atlas",
        "Japanese": "アルカディアンアトラスへようこそ"
      },
      {
        "id": "CH1_02",
        "English": "Arcadia is a vast land, with monsters and dangers!",
        "Portuguese": "Arcadia é uma terra vasta, com monstros e perigos!",
        "Japanese": "アルカディアは広大な地で、モンスターや危険が満載です！"
      },
      {
        "id": "CH1_03",
        "English": "Find him {{player_name}. Find him and save Arcadia. Jonathan will save us all. It is the only way.",
        "Portuguese": "Encontre-o {{player_name}. Encontre-o e salve Arcadia. Jonathan salvará todos nós. É a única maneira.",
        "Japanese": "{{player_name}、彼を見つけてください。彼を見つけてアルカディアを救ってください。ジョナサンが私たち全員を救ってくれます。それが唯一の方法です。"
      }
    ]
    ```
    By doing these translations, the essence of the original strings has been captured in both Portuguese and Japanese thereby making the story engaging for both sets of players."#,
    FieldType::Primitive(TypeValue::String),
    r#"When translating the game strings from English to Portuguese and Japanese, the first concern is to preserve the game world atmospheric impression and the sense of adventure. 

    For ID: CH1_Welcome, the objective is to welcome the player to the game "Arcadian Atlas". The translation should be straightforward without changing the game's title.
    
    For ID: CH1_02, the string introduces the player to the game world's challenges, emphasizing its dangers with the presence of monsters. The tone is designed to evoke excitement and a sense of adventure, which should also be maintained in the translated versions.
    
    For ID: CH1_03, there's a mission to save "Arcadia". The string also introduces a crucial character, "Jonathan", who seems to be the key to salvation. The Portuguese and Japanese versions need to recreate the sense of urgency and responsibility subtly presented in this string.
    
    JSON Output:
    ```json
    [
      {
        "id": "CH1_Welcome",
        "English": "Welcome to Arcadian Atlas",
        "Portuguese": "Bem-vindo ao Arcadian Atlas",
        "Japanese": "アルカディアンアトラスへようこそ"
      },
      {
        "id": "CH1_02",
        "English": "Arcadia is a vast land, with monsters and dangers!",
        "Portuguese": "Arcadia é uma terra vasta, com monstros e perigos!",
        "Japanese": "アルカディアは広大な地で、モンスターや危険が満載です！"
      },
      {
        "id": "CH1_03",
        "English": "Find him {{player_name}. Find him and save Arcadia. Jonathan will save us all. It is the only way.",
        "Portuguese": "Encontre-o {{player_name}. Encontre-o e salve Arcadia. Jonathan salvará todos nós. É a única maneira.",
        "Japanese": "{{player_name}、彼を見つけてください。彼を見つけてアルカディアを救ってください。ジョナサンが私たち全員を救ってくれます。それが唯一の方法です。"
      }
    ]
    ```
    By doing these translations, the essence of the original strings has been captured in both Portuguese and Japanese thereby making the story engaging for both sets of players."#
);
