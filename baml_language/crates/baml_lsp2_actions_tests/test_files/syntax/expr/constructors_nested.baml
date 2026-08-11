class Foo {
  bar Bar
}

class Bar {
  name string
}

let x = Foo { bar: Bar { name: 10 }};

// error: Error validating: Bar.name expected type string, but found int
//   -->  expr/constructors_nested.baml:9
//    |
//  8 |
//  9 | let x = Foo { bar: Bar { name: 10 }};
//    |

//----
//- diagnostics
// E0010
//
//   × top-level `let` bindings are not supported
//    ╭─[expr_constructors_nested.baml:9:1]
//  9 │ let x = Foo { bar: Bar { name: 10 }};
//    · ──────────────────┬──────────────────
//    ·                   ╰── move this binding into a function body
//    ╰────
