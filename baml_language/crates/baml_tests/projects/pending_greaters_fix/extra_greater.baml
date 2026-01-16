// Test case: Extra '>' in generic type should not cause cascading errors
// Bug: pending_greaters counter wasn't reset at type boundaries, causing
// subsequent generic types to be parsed incorrectly.

class MalformedClass {
  // This field has an extra '>' - should report error about unmatched '>'
  bad_field map<string, int>>
  
  // This field should parse correctly without cascading errors
  good_field map<string, int>
  
  // Another well-formed field to verify no cascading
  another_field string
}

// Test nested generics working correctly (no extra >)
class NestedGenerics {
  nested map<string, map<string, int>>
  simple map<string, int>
}

// Function parameter with extra >
function BadParam(x: map<string, int>>) -> string {
  client GPT4
  prompt #"test"#
}

// Function return type with extra >
function BadReturnType() -> map<string, int>> {
  client GPT4
  prompt #"test"#
}

// Type alias with extra >
type BadAlias = map<string, int>>
