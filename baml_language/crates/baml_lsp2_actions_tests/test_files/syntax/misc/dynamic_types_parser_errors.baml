function TypeBuilderFn(from_text: string) -> Resume {
  client "openai/gpt-4o-mini"
  prompt `Hello`
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
// E0010
//
//   × unexpected token
//     ╭─[misc_dynamic_types_parser_errors.baml:40:13]
//  40 │     dynamic Bar {
//     ·             ─┬─
//     ·              ╰── expected `Incomplete 'dynamic' type definition. Use 'dynamic class' or 'dynamic enum' to add properties to types that contain the `@@dynamic` attribute.`, found `identifier`
//     ╰────
// E0010
//
//   × unexpected token
//     ╭─[misc_dynamic_types_parser_errors.baml:40:17]
//  40 │     dynamic Bar {
//     ·                 ┬
//     ·                 ╰── expected `class, enum, dynamic class, dynamic enum, or type alias`, found `'{'`
//     ╰────
// E0010
//
//   × unexpected token
//     ╭─[misc_dynamic_types_parser_errors.baml:41:7]
//  41 │       bar int
//     ·       ─┬─
//     ·        ╰── expected `class, enum, dynamic class, dynamic enum, or type alias`, found `identifier`
//     ╰────
// E0010
//
//   × unexpected token
//     ╭─[misc_dynamic_types_parser_errors.baml:41:11]
//  41 │       bar int
//     ·           ─┬─
//     ·            ╰── expected `class, enum, dynamic class, dynamic enum, or type alias`, found `identifier`
//     ╰────
// E0010
//
//   × unexpected token
//     ╭─[misc_dynamic_types_parser_errors.baml:44:3]
//  44 │   args {
//     ·   ──┬─
//     ·     ╰── expected `top-level declaration`, found `identifier`
//     ╰────
// E0010
//
//   × unexpected token
//     ╭─[misc_dynamic_types_parser_errors.baml:44:8]
//  44 │   args {
//     ·        ┬
//     ·        ╰── expected `top-level declaration`, found `'{'`
//     ╰────
// E0010
//
//   × unexpected token
//     ╭─[misc_dynamic_types_parser_errors.baml:45:5]
//  45 │     from_text "Test"
//     ·     ────┬────
//     ·         ╰── expected `top-level declaration`, found `identifier`
//     ╰────
// E0010
//
//   × unexpected token
//     ╭─[misc_dynamic_types_parser_errors.baml:45:15]
//  45 │     from_text "Test"
//     ·               ┬
//     ·               ╰── expected `top-level declaration`, found `'"'`
//     ╰────
// E0010
//
//   × unexpected token
//     ╭─[misc_dynamic_types_parser_errors.baml:45:16]
//  45 │     from_text "Test"
//     ·                ──┬─
//     ·                  ╰── expected `top-level declaration`, found `identifier`
//     ╰────
// E0010
//
//   × unexpected token
//     ╭─[misc_dynamic_types_parser_errors.baml:45:20]
//  45 │     from_text "Test"
//     ·                    ┬
//     ·                    ╰── expected `top-level declaration`, found `'"'`
//     ╰────
// E0010
//
//   × unexpected token
//     ╭─[misc_dynamic_types_parser_errors.baml:46:3]
//  46 │   }
//     ·   ┬
//     ·   ╰── expected `top-level declaration`, found `'}'`
//     ╰────
// E0010
//
//   × unexpected token
//     ╭─[misc_dynamic_types_parser_errors.baml:47:1]
//  47 │ }
//     · ┬
//     · ╰── expected `top-level declaration`, found `'}'`
//     ╰────
// E0010
//
//   × unexpected token
//     ╭─[misc_dynamic_types_parser_errors.baml:54:5]
//  54 │     dynamic class Bar {
//     ·     ───┬───
//     ·        ╰── expected `type alias name`, found `dynamic`
//     ╰────
// E0002
//
//   × unresolved type: Resume
//    ╭─[misc_dynamic_types_parser_errors.baml:1:46]
//  1 │ function TypeBuilderFn(from_text: string) -> Resume {
//    ·                                              ──────
//    ╰────
// E0002
//
//   × unresolved type: Resume
//    ╭─[misc_dynamic_types_parser_errors.baml:1:1]
//  1 │ function TypeBuilderFn(from_text: string) -> Resume {
//    · ▲
//    ╰────
// E0002
//
//   × unresolved type: Resume
//    ╭─[misc_dynamic_types_parser_errors.baml:1:46]
//  1 │ function TypeBuilderFn(from_text: string) -> Resume {
//    ·                                              ──────
//    ╰────
// E0002
//
//   × unresolved type: Resume
//    ╭─[misc_dynamic_types_parser_errors.baml:1:1]
//  1 │ function TypeBuilderFn(from_text: string) -> Resume {
//    · ▲
//    ╰────
// E0002
//
//   × unresolved type: Resume
//    ╭─[misc_dynamic_types_parser_errors.baml:1:46]
//  1 │ function TypeBuilderFn(from_text: string) -> Resume {
//    ·                                              ──────
//    ╰────
