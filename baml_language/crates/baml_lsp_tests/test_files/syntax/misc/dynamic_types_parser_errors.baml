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
// Error: Expected '}', found type_builder
//     ╭─[ misc_dynamic_types_parser_errors.baml:16:3 ]
//     │
//  16 │   type_builder {
//     │   ──────┬─────  
//     │         ╰─────── Expected '}', found type_builder
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found type_builder
//     ╭─[ misc_dynamic_types_parser_errors.baml:16:3 ]
//     │
//  16 │   type_builder {
//     │   ──────┬─────  
//     │         ╰─────── Expected top-level declaration, found type_builder
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found '{'
//     ╭─[ misc_dynamic_types_parser_errors.baml:16:16 ]
//     │
//  16 │   type_builder {
//     │                ┬  
//     │                ╰── Expected top-level declaration, found '{'
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found dynamic
//     ╭─[ misc_dynamic_types_parser_errors.baml:20:5 ]
//     │
//  20 │     dynamic class Bar {
//     │     ───┬───  
//     │        ╰───── Expected top-level declaration, found dynamic
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found '}'
//     ╭─[ misc_dynamic_types_parser_errors.baml:23:3 ]
//     │
//  23 │   }
//     │   ┬  
//     │   ╰── Expected top-level declaration, found '}'
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found type_builder
//     ╭─[ misc_dynamic_types_parser_errors.baml:24:3 ]
//     │
//  24 │   type_builder {
//     │   ──────┬─────  
//     │         ╰─────── Expected top-level declaration, found type_builder
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found '{'
//     ╭─[ misc_dynamic_types_parser_errors.baml:24:16 ]
//     │
//  24 │   type_builder {
//     │                ┬  
//     │                ╰── Expected top-level declaration, found '{'
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found dynamic
//     ╭─[ misc_dynamic_types_parser_errors.baml:28:5 ]
//     │
//  28 │     dynamic class B {
//     │     ───┬───  
//     │        ╰───── Expected top-level declaration, found dynamic
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found '}'
//     ╭─[ misc_dynamic_types_parser_errors.baml:31:3 ]
//     │
//  31 │   }
//     │   ┬  
//     │   ╰── Expected top-level declaration, found '}'
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found identifier
//     ╭─[ misc_dynamic_types_parser_errors.baml:32:3 ]
//     │
//  32 │   args {
//     │   ──┬─  
//     │     ╰─── Expected top-level declaration, found identifier
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found '{'
//     ╭─[ misc_dynamic_types_parser_errors.baml:32:8 ]
//     │
//  32 │   args {
//     │        ┬  
//     │        ╰── Expected top-level declaration, found '{'
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found identifier
//     ╭─[ misc_dynamic_types_parser_errors.baml:33:5 ]
//     │
//  33 │     from_text "Test"
//     │     ────┬────  
//     │         ╰────── Expected top-level declaration, found identifier
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found '"'
//     ╭─[ misc_dynamic_types_parser_errors.baml:33:15 ]
//     │
//  33 │     from_text "Test"
//     │               ┬  
//     │               ╰── Expected top-level declaration, found '"'
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found identifier
//     ╭─[ misc_dynamic_types_parser_errors.baml:33:16 ]
//     │
//  33 │     from_text "Test"
//     │                ──┬─  
//     │                  ╰─── Expected top-level declaration, found identifier
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found '"'
//     ╭─[ misc_dynamic_types_parser_errors.baml:33:20 ]
//     │
//  33 │     from_text "Test"
//     │                    ┬  
//     │                    ╰── Expected top-level declaration, found '"'
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found '}'
//     ╭─[ misc_dynamic_types_parser_errors.baml:34:3 ]
//     │
//  34 │   }
//     │   ┬  
//     │   ╰── Expected top-level declaration, found '}'
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found '}'
//     ╭─[ misc_dynamic_types_parser_errors.baml:35:1 ]
//     │
//  35 │ }
//     │ ┬  
//     │ ╰── Expected top-level declaration, found '}'
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected '}', found type_builder
//     ╭─[ misc_dynamic_types_parser_errors.baml:39:3 ]
//     │
//  39 │   type_builder {
//     │   ──────┬─────  
//     │         ╰─────── Expected '}', found type_builder
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found type_builder
//     ╭─[ misc_dynamic_types_parser_errors.baml:39:3 ]
//     │
//  39 │   type_builder {
//     │   ──────┬─────  
//     │         ╰─────── Expected top-level declaration, found type_builder
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found '{'
//     ╭─[ misc_dynamic_types_parser_errors.baml:39:16 ]
//     │
//  39 │   type_builder {
//     │                ┬  
//     │                ╰── Expected top-level declaration, found '{'
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found dynamic
//     ╭─[ misc_dynamic_types_parser_errors.baml:40:5 ]
//     │
//  40 │     dynamic Bar {
//     │     ───┬───  
//     │        ╰───── Expected top-level declaration, found dynamic
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found identifier
//     ╭─[ misc_dynamic_types_parser_errors.baml:40:13 ]
//     │
//  40 │     dynamic Bar {
//     │             ─┬─  
//     │              ╰─── Expected top-level declaration, found identifier
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found '{'
//     ╭─[ misc_dynamic_types_parser_errors.baml:40:17 ]
//     │
//  40 │     dynamic Bar {
//     │                 ┬  
//     │                 ╰── Expected top-level declaration, found '{'
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found identifier
//     ╭─[ misc_dynamic_types_parser_errors.baml:41:7 ]
//     │
//  41 │       bar int
//     │       ─┬─  
//     │        ╰─── Expected top-level declaration, found identifier
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found identifier
//     ╭─[ misc_dynamic_types_parser_errors.baml:41:11 ]
//     │
//  41 │       bar int
//     │           ─┬─  
//     │            ╰─── Expected top-level declaration, found identifier
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found '}'
//     ╭─[ misc_dynamic_types_parser_errors.baml:42:5 ]
//     │
//  42 │     }
//     │     ┬  
//     │     ╰── Expected top-level declaration, found '}'
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found '}'
//     ╭─[ misc_dynamic_types_parser_errors.baml:43:3 ]
//     │
//  43 │   }
//     │   ┬  
//     │   ╰── Expected top-level declaration, found '}'
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
// Error: Expected '}', found type_builder
//     ╭─[ misc_dynamic_types_parser_errors.baml:51:3 ]
//     │
//  51 │   type_builder {
//     │   ──────┬─────  
//     │         ╰─────── Expected '}', found type_builder
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found type_builder
//     ╭─[ misc_dynamic_types_parser_errors.baml:51:3 ]
//     │
//  51 │   type_builder {
//     │   ──────┬─────  
//     │         ╰─────── Expected top-level declaration, found type_builder
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found '{'
//     ╭─[ misc_dynamic_types_parser_errors.baml:51:16 ]
//     │
//  51 │   type_builder {
//     │                ┬  
//     │                ╰── Expected top-level declaration, found '{'
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
// Error: Expected top-level declaration, found dynamic
//     ╭─[ misc_dynamic_types_parser_errors.baml:54:5 ]
//     │
//  54 │     dynamic class Bar {
//     │     ───┬───  
//     │        ╰───── Expected top-level declaration, found dynamic
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found '}'
//     ╭─[ misc_dynamic_types_parser_errors.baml:57:3 ]
//     │
//  57 │   }
//     │   ┬  
//     │   ╰── Expected top-level declaration, found '}'
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found identifier
//     ╭─[ misc_dynamic_types_parser_errors.baml:58:3 ]
//     │
//  58 │   args {
//     │   ──┬─  
//     │     ╰─── Expected top-level declaration, found identifier
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found '{'
//     ╭─[ misc_dynamic_types_parser_errors.baml:58:8 ]
//     │
//  58 │   args {
//     │        ┬  
//     │        ╰── Expected top-level declaration, found '{'
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found identifier
//     ╭─[ misc_dynamic_types_parser_errors.baml:59:5 ]
//     │
//  59 │     from_text "Test"
//     │     ────┬────  
//     │         ╰────── Expected top-level declaration, found identifier
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found '"'
//     ╭─[ misc_dynamic_types_parser_errors.baml:59:15 ]
//     │
//  59 │     from_text "Test"
//     │               ┬  
//     │               ╰── Expected top-level declaration, found '"'
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found identifier
//     ╭─[ misc_dynamic_types_parser_errors.baml:59:16 ]
//     │
//  59 │     from_text "Test"
//     │                ──┬─  
//     │                  ╰─── Expected top-level declaration, found identifier
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found '"'
//     ╭─[ misc_dynamic_types_parser_errors.baml:59:20 ]
//     │
//  59 │     from_text "Test"
//     │                    ┬  
//     │                    ╰── Expected top-level declaration, found '"'
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found '}'
//     ╭─[ misc_dynamic_types_parser_errors.baml:60:3 ]
//     │
//  60 │   }
//     │   ┬  
//     │   ╰── Expected top-level declaration, found '}'
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found '}'
//     ╭─[ misc_dynamic_types_parser_errors.baml:61:1 ]
//     │
//  61 │ }
//     │ ┬  
//     │ ╰── Expected top-level declaration, found '}'
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Duplicate class 'Bar'
//    ╭─[ misc_dynamic_types_parser_errors.baml:1:1 ]
//    │
//  1 │ function TypeBuilderFn(from_text: string) -> Resume {
//    │ │ 
//    │ ╰─ class 'Bar' defined in misc_dynamic_types_parser_errors.baml
//    │ │ 
//    │ ╰─ 'Bar' previously defined in misc_dynamic_types_parser_errors.baml
//    │ 
//    │ Note: Error code: E0011
// ───╯
// Error: Duplicate class 'Bar'
//    ╭─[ misc_dynamic_types_parser_errors.baml:1:1 ]
//    │
//  1 │ function TypeBuilderFn(from_text: string) -> Resume {
//    │ │ 
//    │ ╰─ class 'Bar' defined in misc_dynamic_types_parser_errors.baml
//    │ │ 
//    │ ╰─ 'Bar' previously defined in misc_dynamic_types_parser_errors.baml
//    │ 
//    │ Note: Error code: E0011
// ───╯
// Error: Duplicate class 'Foo'
//    ╭─[ misc_dynamic_types_parser_errors.baml:1:1 ]
//    │
//  1 │ function TypeBuilderFn(from_text: string) -> Resume {
//    │ │ 
//    │ ╰─ class 'Foo' defined in misc_dynamic_types_parser_errors.baml
//    │ │ 
//    │ ╰─ 'Foo' previously defined in misc_dynamic_types_parser_errors.baml
//    │ 
//    │ Note: Error code: E0011
// ───╯
// Error: Unknown type Resume
//    ╭─[ misc_dynamic_types_parser_errors.baml:1:1 ]
//    │
//  1 │ function TypeBuilderFn(from_text: string) -> Resume {
//    │ │ 
//    │ ╰─ Unknown type Resume
//    │ 
//    │ Note: Error code: E0002
// ───╯
