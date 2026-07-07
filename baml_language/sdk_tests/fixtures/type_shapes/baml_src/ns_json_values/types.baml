// Coverage for the stdlib `json` type (the `json` keyword resolves to
// `baml.json.json`, a recursive JSON-shaped union). Each `return_json_*`
// function returns a differently-shaped value whose declared return type
// is `json`; `round_trip_json` exercises the encode/decode pair over the
// same shapes, and `JsonContainer` puts `json` in a class field position.

class JsonContainer {
  data json
}

function return_json_null() -> json {
  null
}

function return_json_bool() -> json {
  true
}

function return_json_int() -> json {
  42
}

function return_json_float() -> json {
  3.14
}

function return_json_string() -> json {
  "hello"
}

function return_json_array() -> json {
  [1, 2, 3]
}

function return_json_object() -> json {
  {"key": "value"}
}

function return_json_nested() -> json {
  {"a": 1, "b": [2, 3], "c": {"nested": null}}
}

function round_trip_json(j: json) -> json {
  j
}

function round_trip_json_container(c: JsonContainer) -> JsonContainer {
  c
}
