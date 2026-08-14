enum Test {
    A int | string
}

// error: Error validating: Unexpected type specified for value `A`
//   -->  enum/invalid_value_expr.baml:2
//    |
//  1 | enum Test {
//  2 |     A int | string
//  3 | }
//    |

//----
//- diagnostics
// E0010
//
//   × invalid syntax
//    ╭─[enum_invalid_value_expr.baml:2:7]
//  2 │     A int | string
//    ·       ──────┬─────
//    ·             ╰── enum variants cannot have type annotations
//    ╰────
