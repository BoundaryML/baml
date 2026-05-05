type json = null | bool | int | float | string | json[] | map<string, json>

class JsonParseError {
  message string
}
class JsonDecodeError {
  message string
  path string
}
class JsonSerializationError {
  message string
  path string
  reason string
}
