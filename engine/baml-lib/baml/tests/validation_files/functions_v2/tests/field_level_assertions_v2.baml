// Test that field-level assertions are not allowed on test block fields

function TestFunction(input: string) -> string {
  client "openai/gpt-4"
  prompt #"Test function"#
}

test MyTest {
  functions [TestFunction] @assert({{ true }})
  args {
    input "hello"
  }
}

// error: Error validating: @assert is not allowed on test fields. Use @@assert at the test block level instead.
//   -->  functions_v2/tests/field_level_assertions_v2.baml:9
//    | 
//  8 | test MyTest {
//  9 |   functions [TestFunction] @assert({{ true }})
//    | 
