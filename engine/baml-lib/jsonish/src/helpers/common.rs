use baml_types::{FieldType, LiteralValue};
use internal_baml_jinja::types::{Builder, OutputFormatContent};

pub const CLASS_SCHEMA: &str = r#"
class Book {
    title string
    author string
    year int
    tags string[]
    ratings Rating[]
}

class Rating {
    score int
    reviewer string
    date string
}
"#;

pub const UNION_SCHEMA: &str = r#"
class TextContent {
    text string
}

class ImageContent {
    url string
    width int
    height int
}

class VideoContent {
    url string
    duration int
}

class AudioContent {
    type string
    url string
    duration int
}

type JSONValue = int | float | bool | string | null | JSONValue[] | map<string, JSONValue>
"#;

pub const JSON_STRING: &str = r#"
    {
        "number": 1,
        "string": "test",
        "bool": true,
        "list": [1, 2, 3],
        "object": {
            "number": 1,
            "string": "test",
            "bool": true,
            "list": [1, 2, 3]
        },
        "json": {
            "number": 1,
            "string": "test",
            "bool": true,
            "list": [1, 2, 3],
            "object": {
                "number": 1,
                "string": "test",
                "bool": true,
                "list": [1, 2, 3]
            }
        }
    }
"#;
