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
// E0001
//
//   × mismatched types
//    ╭─[expr_constructors_nested.baml:9:32]
//  9 │ let x = Foo { bar: Bar { name: 10 }};
//    ·                                ─┬
//    ·                                 ╰── expected `string`, found `10`
//    ╰────
