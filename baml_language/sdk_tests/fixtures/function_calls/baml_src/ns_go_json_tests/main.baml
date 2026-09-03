// Canonical stdlib JSON must be the only recursive alias projected as Go any.
type JsonAlias = baml.json.json
type UserRecursiveJson = null | bool | int | float | string | UserRecursiveJson[] | map<string, UserRecursiveJson>

class JsonBox {
  payload baml.json.json
  list baml.json.json[]
  mapping map<string, baml.json.json>
  nullable baml.json.json?
}

function round_trip_json(value: baml.json.json) -> baml.json.json {
  value
}

function round_trip_json_alias(value: JsonAlias) -> JsonAlias {
  value
}

function round_trip_json_list(value: baml.json.json[]) -> baml.json.json[] {
  value
}

function round_trip_json_map(value: map<string, baml.json.json>) -> map<string, baml.json.json> {
  value
}

function round_trip_json_box(value: JsonBox) -> JsonBox {
  value
}

function default_json(value: baml.json.json = null) -> baml.json.json {
  value
}

function call_json_callback(callback: (baml.json.json) -> baml.json.json, value: baml.json.json) -> baml.json.json {
  callback(value)
}

function round_trip_json_or_image(value: baml.json.json | image) -> baml.json.json | image {
  value
}

function round_trip_json_alias_or_image(value: JsonAlias | image) -> JsonAlias | image {
  value
}

function round_trip_json_box_or_image(value: JsonBox | image) -> JsonBox | image {
  value
}

function round_trip_json_box_dynamic(value: JsonBox | image | audio | video) -> JsonBox | image | audio | video {
  value
}

function unsupported_user_recursive(value: UserRecursiveJson) -> UserRecursiveJson {
  value
}

// Host-supplied json must materialize with `json` container typing so typed
// narrowing (`match`, and therefore `baml.json.path` / `path_or`) behaves
// identically to BAML-born `baml.json.parse` values.
function json_kind(value: baml.json.json) -> string {
  match (value) {
    let m: map<string, baml.json.json> => "object",
    let arr: baml.json.json[] => "array",
    let s: string => "string",
    _ => "other",
  }
}

function json_path_string(value: baml.json.json, selector: string) -> string throws baml.json.PathError {
  baml.json.path<string>(value, selector)
}

function json_path_string_or(value: baml.json.json, selector: string, default: string) -> string throws never {
  baml.json.path_or<string>(value, selector, default)
}

// A host callback's json return value must materialize with `json` container
// typing too — it converts on the host-return path, not the argument path.
function json_callback_kind(callback: (baml.json.json) -> baml.json.json, value: baml.json.json) -> string {
  json_kind(callback(value))
}
