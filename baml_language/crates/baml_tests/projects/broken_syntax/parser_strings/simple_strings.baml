// Test basic string parsing in attributes and types
class Messages {
  greeting string @alias("hello")
  empty string @description("")
  with_spaces string @alias("hello world")
  with_escapes string @description("line1\nline2\ttab")
  with_quotes string @alias("She said \"hello\"")
}

// String literal union types
function GetRole() -> "admin" | "user" | "guest" {
  return "user";
}
