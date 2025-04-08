function TypeBuilderFn() -> string { 
  client "openai/gpt-4o-mini"
  prompt #"Hello"#
}

class DynamicClass {
  a string
  b string
  @@dynamic
}

test AttemptToMakeClassInfinitelyRecursive {
  functions [TypeBuilderFn]
  type_builder {
    dynamic class DynamicClass {
      cycle DynamicClass
    }
  }
  args {
    from_text "Test"
  }
}

// error: Error validating: These classes form a dependency cycle: DynamicClass
//   -->  tests/dynamic_types_internal_cycle_errors.baml:15
//    | 
// 14 |   type_builder {
// 15 |     dynamic class DynamicClass {
// 16 |       cycle DynamicClass
// 17 |     }
//    | 
