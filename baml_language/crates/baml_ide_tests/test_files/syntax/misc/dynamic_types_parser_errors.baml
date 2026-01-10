function TypeBuilderFn(from_text: string) -> Resume { 
  client "openai/gpt-4o-mini"
  prompt #"Hello"#
  type_builder {
    class Foo {
      foo string
    }
    dynamic class Bar {
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
    dynamic class Bar {
      bar int
    }
  }
  type_builder {
    class A {
      a string
    }
    dynamic class B {
      b int
    }
  }
  args {
    from_text "Test"
  }
}

test IncompleteDynamicDef {
  functions [TypeBuilderFn]
  type_builder {
    dynamic Bar {
      bar int
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

    dynamic class Bar {
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
//  8 |     dynamic class Bar {
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
// 28 |     dynamic class B {
// 29 |       b int
// 30 |     }
// 31 |   }
//    | 
// error: Error validating: Incomplete 'dynamic' type definition. Use 'dynamic class' or 'dynamic enum' to add properties to types that contain the `@@dynamic` attribute.
//   -->  tests/dynamic_types_parser_errors.baml:40
//    | 
// 39 |   type_builder {
// 40 |     dynamic Bar {
//    | 
// error: Error validating: Syntax error in type builder block
//   -->  tests/dynamic_types_parser_errors.baml:52
//    | 
// 51 |   type_builder {
// 52 |     type
// 53 | 
//    |

//----
//- diagnostics
// Error: Expected '}', found type_builder
//    ╭─[ misc_dynamic_types_parser_errors.baml:4:3 ]
//    │
//  4 │   type_builder {
//    │   ──────┬─────  
//    │         ╰─────── Expected '}', found type_builder
//    │ 
//    │ Note: Error code: E0010
// ───╯
// Error: Expected top-level declaration, found type_builder
//    ╭─[ misc_dynamic_types_parser_errors.baml:4:3 ]
//    │
//  4 │   type_builder {
//    │   ──────┬─────  
//    │         ╰─────── Expected top-level declaration, found type_builder
//    │ 
//    │ Note: Error code: E0010
// ───╯
// Error: Expected top-level declaration, found '{'
//    ╭─[ misc_dynamic_types_parser_errors.baml:4:16 ]
//    │
//  4 │   type_builder {
//    │                ┬  
//    │                ╰── Expected top-level declaration, found '{'
//    │ 
//    │ Note: Error code: E0010
// ───╯
// Error: Expected top-level declaration, found dynamic
//    ╭─[ misc_dynamic_types_parser_errors.baml:8:5 ]
//    │
//  8 │     dynamic class Bar {
//    │     ───┬───  
//    │        ╰───── Expected top-level declaration, found dynamic
//    │ 
//    │ Note: Error code: E0010
// ───╯
// Error: Expected top-level declaration, found '}'
//     ╭─[ misc_dynamic_types_parser_errors.baml:11:3 ]
//     │
//  11 │   }
//     │   ┬  
//     │   ╰── Expected top-level declaration, found '}'
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found '}'
//     ╭─[ misc_dynamic_types_parser_errors.baml:12:1 ]
//     │
//  12 │ }
//     │ ┬  
//     │ ╰── Expected top-level declaration, found '}'
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected class or enum after 'dynamic', found identifier
//     ╭─[ misc_dynamic_types_parser_errors.baml:40:13 ]
//     │
//  40 │     dynamic Bar {
//     │             ─┬─  
//     │              ╰─── Expected class or enum after 'dynamic', found identifier
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected class, enum, dynamic class, dynamic enum, or type alias, found identifier
//     ╭─[ misc_dynamic_types_parser_errors.baml:40:13 ]
//     │
//  40 │     dynamic Bar {
//     │             ─┬─  
//     │              ╰─── Expected class, enum, dynamic class, dynamic enum, or type alias, found identifier
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected class, enum, dynamic class, dynamic enum, or type alias, found '{'
//     ╭─[ misc_dynamic_types_parser_errors.baml:40:17 ]
//     │
//  40 │     dynamic Bar {
//     │                 ┬  
//     │                 ╰── Expected class, enum, dynamic class, dynamic enum, or type alias, found '{'
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected class, enum, dynamic class, dynamic enum, or type alias, found identifier
//     ╭─[ misc_dynamic_types_parser_errors.baml:41:7 ]
//     │
//  41 │       bar int
//     │       ─┬─  
//     │        ╰─── Expected class, enum, dynamic class, dynamic enum, or type alias, found identifier
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected class, enum, dynamic class, dynamic enum, or type alias, found identifier
//     ╭─[ misc_dynamic_types_parser_errors.baml:41:11 ]
//     │
//  41 │       bar int
//     │           ─┬─  
//     │            ╰─── Expected class, enum, dynamic class, dynamic enum, or type alias, found identifier
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found identifier
//     ╭─[ misc_dynamic_types_parser_errors.baml:44:3 ]
//     │
//  44 │   args {
//     │   ──┬─  
//     │     ╰─── Expected top-level declaration, found identifier
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found '{'
//     ╭─[ misc_dynamic_types_parser_errors.baml:44:8 ]
//     │
//  44 │   args {
//     │        ┬  
//     │        ╰── Expected top-level declaration, found '{'
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found identifier
//     ╭─[ misc_dynamic_types_parser_errors.baml:45:5 ]
//     │
//  45 │     from_text "Test"
//     │     ────┬────  
//     │         ╰────── Expected top-level declaration, found identifier
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found '"'
//     ╭─[ misc_dynamic_types_parser_errors.baml:45:15 ]
//     │
//  45 │     from_text "Test"
//     │               ┬  
//     │               ╰── Expected top-level declaration, found '"'
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found identifier
//     ╭─[ misc_dynamic_types_parser_errors.baml:45:16 ]
//     │
//  45 │     from_text "Test"
//     │                ──┬─  
//     │                  ╰─── Expected top-level declaration, found identifier
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found '"'
//     ╭─[ misc_dynamic_types_parser_errors.baml:45:20 ]
//     │
//  45 │     from_text "Test"
//     │                    ┬  
//     │                    ╰── Expected top-level declaration, found '"'
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found '}'
//     ╭─[ misc_dynamic_types_parser_errors.baml:46:3 ]
//     │
//  46 │   }
//     │   ┬  
//     │   ╰── Expected top-level declaration, found '}'
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found '}'
//     ╭─[ misc_dynamic_types_parser_errors.baml:47:1 ]
//     │
//  47 │ }
//     │ ┬  
//     │ ╰── Expected top-level declaration, found '}'
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected type alias name, found dynamic
//     ╭─[ misc_dynamic_types_parser_errors.baml:54:5 ]
//     │
//  54 │     dynamic class Bar {
//     │     ───┬───  
//     │        ╰───── Expected type alias name, found dynamic
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected '=', found dynamic
//     ╭─[ misc_dynamic_types_parser_errors.baml:54:5 ]
//     │
//  54 │     dynamic class Bar {
//     │     ───┬───  
//     │        ╰───── Expected '=', found dynamic
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected type, found dynamic
//     ╭─[ misc_dynamic_types_parser_errors.baml:54:5 ]
//     │
//  54 │     dynamic class Bar {
//     │     ───┬───  
//     │        ╰───── Expected type, found dynamic
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Missing `args` property
//     ╭─[ misc_dynamic_types_parser_errors.baml:37:6 ]
//     │
//  37 │ test IncompleteDynamicDef {
//     │      ──────────┬─────────  
//     │                ╰─────────── Missing `args` property
//     │ 
//     │ Note: Error code: E0035
// ────╯
