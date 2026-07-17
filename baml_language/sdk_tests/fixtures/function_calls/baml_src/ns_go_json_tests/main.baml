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
