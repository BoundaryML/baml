// Phase 1: Verify that the `json` keyword resolves to `baml.json.json` and
// that class fields, variable declarations, and match arms all type-check.

class Wrapper {
  data json
}

function MakeWrapper() -> Wrapper {
  Wrapper { data: null }
}

function ReturnNull() -> json {
  null
}

function ReturnBool() -> json {
  true
}

function ReturnInt() -> json {
  42
}

function ReturnFloat() -> json {
  3.14
}

function ReturnString() -> json {
  "hello"
}

function ReturnArray() -> json {
  [1, 2, 3]
}

function ReturnMap() -> json {
  {"key": "value"}
}

function MatchJson(j: json) -> string {
  match (j) {
    null => "null",
    b: bool => "bool",
    i: int => "int",
    f: float => "float",
    s: string => "string",
    arr: json[] => "array",
    obj: map<string, json> => "map"
  }
}
