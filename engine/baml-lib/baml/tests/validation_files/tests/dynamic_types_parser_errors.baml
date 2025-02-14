function TypeBuilderFn(from_text: string) -> Resume { 
  client "openai/gpt-4o-mini"
  prompt #"Hello"#
  type_builder {
    class Foo {
      foo string
    }
    dynamic Bar {
      bar int
    }
  }
}

test MultipleTypeBuilderBlocks {
  functions [TypeBuilderFn]
  type_builder {
    class Foo {
      foo string
    }
    dynamic Bar {
      bar int
    }
  }
  type_builder {
    class A {
      a string
    }
    dynamic B {
      b int
    }
  }
  args {
    from_text "Test"
  }
}

test IncompleteSyntax {
  functions [TypeBuilderFn]
  type_builder {
    type

    dynamic Bar {
      bar int
    }
  }
  args {
    from_text "Test"
  }
}

// error: Error validating: Only tests may have a type_builder block.
//   -->  tests/dynamic_types_parser_errors.baml:4
//    | 
//  3 |   prompt #"Hello"#
//  4 |   type_builder {
//  5 |     class Foo {
//  6 |       foo string
//  7 |     }
//  8 |     dynamic Bar {
//  9 |       bar int
// 10 |     }
// 11 |   }
//    | 
// error: Error validating: Definition of multiple `type_builder` blocks in the same parent block
//   -->  tests/dynamic_types_parser_errors.baml:24
//    | 
// 23 |   }
// 24 |   type_builder {
// 25 |     class A {
// 26 |       a string
// 27 |     }
// 28 |     dynamic B {
// 29 |       b int
// 30 |     }
// 31 |   }
//    | 
// error: Error validating: Syntax error in type builder block
//   -->  tests/dynamic_types_parser_errors.baml:40
//    | 
// 39 |   type_builder {
// 40 |     type
// 41 | 
//    | 
