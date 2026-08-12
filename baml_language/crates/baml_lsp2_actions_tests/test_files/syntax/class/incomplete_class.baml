class InterfaceThree {
  iface int
  foo
}

// error: Error validating: No type specified for field `foo`
//   -->  class/incomplete_class.baml:3
//    |
//  2 |   iface int
//  3 |   foo
//  4 | }
//    |

//----
//- diagnostics
// E0010
//
//   × invalid syntax
//    ╭─[class_incomplete_class.baml:3:3]
//  3 │   foo
//    ·   ─┬─
//    ·    ╰── field 'foo' is missing a type annotation
//    ╰────
