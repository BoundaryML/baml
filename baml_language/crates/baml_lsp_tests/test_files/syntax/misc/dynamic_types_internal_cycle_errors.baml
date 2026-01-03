function TypeBuilderFn() -> string { 
  client "openai/gpt-4o-mini"
  prompt #"Hello"#
}

class DynamicClass {
  a string
  b string
  @@dynamic
}

test AttemptToMakeClassInfinitelyRecursive {
  functions [TypeBuilderFn]
  type_builder {
    dynamic class DynamicClass {
      cycle DynamicClass
    }
  }
  args {
    from_text "Test"
  }
}

// error: Error validating: These classes form a dependency cycle: DynamicClass
//   -->  tests/dynamic_types_internal_cycle_errors.baml:15
//    | 
// 14 |   type_builder {
// 15 |     dynamic class DynamicClass {
// 16 |       cycle DynamicClass
// 17 |     }
//    |

//----
//- diagnostics
// Error: Expected RBrace, found TypeBuilder
//     ╭─[ 0:14:3 ]
//     │
//  14 │   type_builder {
//     │   ──────┬─────  
//     │         ╰─────── Expected RBrace, found TypeBuilder
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found TypeBuilder
//     ╭─[ 0:14:3 ]
//     │
//  14 │   type_builder {
//     │   ──────┬─────  
//     │         ╰─────── Expected top-level declaration, found TypeBuilder
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found LBrace
//     ╭─[ 0:14:16 ]
//     │
//  14 │   type_builder {
//     │                ┬  
//     │                ╰── Expected top-level declaration, found LBrace
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found Dynamic
//     ╭─[ 0:15:5 ]
//     │
//  15 │     dynamic class DynamicClass {
//     │     ───┬───  
//     │        ╰───── Expected top-level declaration, found Dynamic
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found RBrace
//     ╭─[ 0:18:3 ]
//     │
//  18 │   }
//     │   ┬  
//     │   ╰── Expected top-level declaration, found RBrace
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found Word
//     ╭─[ 0:19:3 ]
//     │
//  19 │   args {
//     │   ──┬─  
//     │     ╰─── Expected top-level declaration, found Word
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found LBrace
//     ╭─[ 0:19:8 ]
//     │
//  19 │   args {
//     │        ┬  
//     │        ╰── Expected top-level declaration, found LBrace
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found Word
//     ╭─[ 0:20:5 ]
//     │
//  20 │     from_text "Test"
//     │     ────┬────  
//     │         ╰────── Expected top-level declaration, found Word
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found Quote
//     ╭─[ 0:20:15 ]
//     │
//  20 │     from_text "Test"
//     │               ┬  
//     │               ╰── Expected top-level declaration, found Quote
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found Word
//     ╭─[ 0:20:16 ]
//     │
//  20 │     from_text "Test"
//     │                ──┬─  
//     │                  ╰─── Expected top-level declaration, found Word
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found Quote
//     ╭─[ 0:20:20 ]
//     │
//  20 │     from_text "Test"
//     │                    ┬  
//     │                    ╰── Expected top-level declaration, found Quote
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found RBrace
//     ╭─[ 0:21:3 ]
//     │
//  21 │   }
//     │   ┬  
//     │   ╰── Expected top-level declaration, found RBrace
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found RBrace
//     ╭─[ 0:22:1 ]
//     │
//  22 │ }
//     │ ┬  
//     │ ╰── Expected top-level declaration, found RBrace
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Duplicate class 'DynamicClass'
//    ╭─[ 0:1:1 ]
//    │
//  1 │ function TypeBuilderFn() -> string {
//    │ │ 
//    │ ╰─ class 'DynamicClass' defined in misc_dynamic_types_internal_cycle_errors.baml
//    │ │ 
//    │ ╰─ 'DynamicClass' previously defined in misc_dynamic_types_internal_cycle_errors.baml
//    │ 
//    │ Note: Error code: E0011
// ───╯
