function ComplexFunction(data: string) -> string {
  client "openai/gpt-3.5-turbo"
  prompt #"Process {{data}}"#
}

test TestFieldLevelCheck {
  functions [ComplexFunction]
  args {
    data "value"
  }
  @check(this != null)  // Should error - use @@check instead
}

// error: Error parsing attribute "check": Test checks must use block-level syntax '@@check' instead of '@check'. Block-level attributes apply to the entire test result.
//   -->  tests/field_level_check.baml:11
//    | 
// 10 |   }
// 11 |   @check(this != null)  // Should error - use @@check instead
//    | 
