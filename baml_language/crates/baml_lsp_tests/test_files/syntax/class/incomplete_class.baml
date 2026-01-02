class InterfaceThree {
  interface int
  foo
}

// error: Error validating: No type specified for field `foo`
//   -->  class/incomplete_class.baml:3
//    | 
//  2 |   interface int
//  3 |   foo
//  4 | }
//    |

//----
//- diagnostics
// Error: field 'foo' is missing a type annotation
//    ╭─[ 0:3:3 ]
//    │
//  3 │   foo
//    │   ─┬─  
//    │    ╰─── field 'foo' is missing a type annotation
//    │ 
//    │ Note: Error code: E0010
// ───╯
