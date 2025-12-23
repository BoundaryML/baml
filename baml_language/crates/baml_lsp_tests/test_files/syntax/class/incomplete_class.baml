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
// Error: Expected type, found RBrace
//    ╭─[ 0:4:1 ]
//    │
//  4 │ }
//    │ ┬  
//    │ ╰── Expected type, found RBrace
//    │ 
//    │ Note: Error code: E0010
// ───╯
