use super::*;

test_partial_deserializer_streaming!(
    toon_unfinished_field_respects_stream_done,
    "class Secret {\n  value string @stream.done\n}",
    "value: unfinished",
    TypeIR::class("Secret"),
    {"value": null}
);

test_deserializer!(
    toon_finished_field_respects_stream_done,
    "class Secret {\n  value string @stream.done\n}",
    "value: finished",
    TypeIR::class("Secret"),
    {"value": "finished"}
);

const TOON_TYPES: &str = r#"
enum Status {
  ACTIVE
  PAUSED
}

class Address {
  city string
}

class Result {
  name string
  status Status
  tags string[]
  metadata map<string, int>
  address Address?
  identifier string | int
}

class People {
  people Person[]
}

class Person {
  name string
  age int
}
"#;

test_deserializer!(
    parses_toon_class_with_enum_list_map_optional_and_union,
    TOON_TYPES,
    r#"name: Ada
status: ACTIVE
tags[2]: math,code
metadata:
  score: 10
address:
  city: London
identifier: 42"#,
    TypeIR::class("Result"),
    {
        "name": "Ada",
        "status": "ACTIVE",
        "tags": ["math", "code"],
        "metadata": { "score": 10 },
        "address": { "city": "London" },
        "identifier": 42
    }
);

test_deserializer!(
    parses_toon_tabular_class_list,
    TOON_TYPES,
    r#"people[2]{name,age}:
  Ada,36
  Grace,37"#,
    TypeIR::class("People"),
    {
        "people": [
            { "name": "Ada", "age": 36 },
            { "name": "Grace", "age": 37 }
        ]
    }
);

test_deserializer!(
    preserves_tabular_rows_that_start_with_slashes,
    TOON_TYPES,
    r#"people[2]{name,age}:
  // echoed description
  //foo,7
  Ada,36"#,
    TypeIR::class("People"),
    {
        "people": [
            { "name": "//foo", "age": 7 },
            { "name": "Ada", "age": 36 }
        ]
    }
);

test_deserializer!(
    parses_toon_after_preamble_and_recovers_bad_length,
    TOON_TYPES,
    r#"Here is the requested result:
name: Ada
status: PAUSED
tags[3]: math,code
metadata:
  score: 10
address: null
identifier: baml"#,
    TypeIR::class("Result"),
    {
        "name": "Ada",
        "status": "PAUSED",
        "tags": ["math", "code"],
        "metadata": { "score": 10 },
        "address": null,
        "identifier": "baml"
    }
);
