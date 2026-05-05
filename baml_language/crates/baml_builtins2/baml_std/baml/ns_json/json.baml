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

//baml:mut_vm
function parse(s string) -> json throws JsonParseError {
  $rust_function
}

//baml:mut_vm
function stringify(j json) -> string {
  $rust_function
}

//baml:mut_vm
function stringify_pretty(j json) -> string {
  $rust_function
}
