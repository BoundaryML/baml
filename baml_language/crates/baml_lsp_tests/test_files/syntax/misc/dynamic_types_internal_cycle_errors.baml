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
// Error: Expected '}', found type_builder
//     ╭─[ 0:14:3 ]
//     │
//  14 │   type_builder {
//     │   ──────┬─────  
//     │         ╰─────── Expected '}', found type_builder
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found type_builder
//     ╭─[ 0:14:3 ]
//     │
//  14 │   type_builder {
//     │   ──────┬─────  
//     │         ╰─────── Expected top-level declaration, found type_builder
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found '{'
//     ╭─[ 0:14:16 ]
//     │
//  14 │   type_builder {
//     │                ┬  
//     │                ╰── Expected top-level declaration, found '{'
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found dynamic
//     ╭─[ 0:15:5 ]
//     │
//  15 │     dynamic class DynamicClass {
//     │     ───┬───  
//     │        ╰───── Expected top-level declaration, found dynamic
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found '}'
//     ╭─[ 0:18:3 ]
//     │
//  18 │   }
//     │   ┬  
//     │   ╰── Expected top-level declaration, found '}'
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found identifier
//     ╭─[ 0:19:3 ]
//     │
//  19 │   args {
//     │   ──┬─  
//     │     ╰─── Expected top-level declaration, found identifier
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found '{'
//     ╭─[ 0:19:8 ]
//     │
//  19 │   args {
//     │        ┬  
//     │        ╰── Expected top-level declaration, found '{'
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found identifier
//     ╭─[ 0:20:5 ]
//     │
//  20 │     from_text "Test"
//     │     ────┬────  
//     │         ╰────── Expected top-level declaration, found identifier
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found '"'
//     ╭─[ 0:20:15 ]
//     │
//  20 │     from_text "Test"
//     │               ┬  
//     │               ╰── Expected top-level declaration, found '"'
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found identifier
//     ╭─[ 0:20:16 ]
//     │
//  20 │     from_text "Test"
//     │                ──┬─  
//     │                  ╰─── Expected top-level declaration, found identifier
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found '"'
//     ╭─[ 0:20:20 ]
//     │
//  20 │     from_text "Test"
//     │                    ┬  
//     │                    ╰── Expected top-level declaration, found '"'
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found '}'
//     ╭─[ 0:21:3 ]
//     │
//  21 │   }
//     │   ┬  
//     │   ╰── Expected top-level declaration, found '}'
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found '}'
//     ╭─[ 0:22:1 ]
//     │
//  22 │ }
//     │ ┬  
//     │ ╰── Expected top-level declaration, found '}'
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
