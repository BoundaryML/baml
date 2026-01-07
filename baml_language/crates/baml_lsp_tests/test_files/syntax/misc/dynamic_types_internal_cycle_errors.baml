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
//     ╭─[ misc_dynamic_types_internal_cycle_errors.baml:14:3 ]
//     │
//  14 │   type_builder {
//     │   ──────┬─────  
//     │         ╰─────── Expected '}', found type_builder
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found type_builder
//     ╭─[ misc_dynamic_types_internal_cycle_errors.baml:14:3 ]
//     │
//  14 │   type_builder {
//     │   ──────┬─────  
//     │         ╰─────── Expected top-level declaration, found type_builder
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found '{'
//     ╭─[ misc_dynamic_types_internal_cycle_errors.baml:14:16 ]
//     │
//  14 │   type_builder {
//     │                ┬  
//     │                ╰── Expected top-level declaration, found '{'
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found dynamic
//     ╭─[ misc_dynamic_types_internal_cycle_errors.baml:15:5 ]
//     │
//  15 │     dynamic class DynamicClass {
//     │     ───┬───  
//     │        ╰───── Expected top-level declaration, found dynamic
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found '}'
//     ╭─[ misc_dynamic_types_internal_cycle_errors.baml:18:3 ]
//     │
//  18 │   }
//     │   ┬  
//     │   ╰── Expected top-level declaration, found '}'
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found identifier
//     ╭─[ misc_dynamic_types_internal_cycle_errors.baml:19:3 ]
//     │
//  19 │   args {
//     │   ──┬─  
//     │     ╰─── Expected top-level declaration, found identifier
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found '{'
//     ╭─[ misc_dynamic_types_internal_cycle_errors.baml:19:8 ]
//     │
//  19 │   args {
//     │        ┬  
//     │        ╰── Expected top-level declaration, found '{'
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found identifier
//     ╭─[ misc_dynamic_types_internal_cycle_errors.baml:20:5 ]
//     │
//  20 │     from_text "Test"
//     │     ────┬────  
//     │         ╰────── Expected top-level declaration, found identifier
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found '"'
//     ╭─[ misc_dynamic_types_internal_cycle_errors.baml:20:15 ]
//     │
//  20 │     from_text "Test"
//     │               ┬  
//     │               ╰── Expected top-level declaration, found '"'
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found identifier
//     ╭─[ misc_dynamic_types_internal_cycle_errors.baml:20:16 ]
//     │
//  20 │     from_text "Test"
//     │                ──┬─  
//     │                  ╰─── Expected top-level declaration, found identifier
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found '"'
//     ╭─[ misc_dynamic_types_internal_cycle_errors.baml:20:20 ]
//     │
//  20 │     from_text "Test"
//     │                    ┬  
//     │                    ╰── Expected top-level declaration, found '"'
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found '}'
//     ╭─[ misc_dynamic_types_internal_cycle_errors.baml:21:3 ]
//     │
//  21 │   }
//     │   ┬  
//     │   ╰── Expected top-level declaration, found '}'
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found '}'
//     ╭─[ misc_dynamic_types_internal_cycle_errors.baml:22:1 ]
//     │
//  22 │ }
//     │ ┬  
//     │ ╰── Expected top-level declaration, found '}'
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Duplicate class `DynamicClass`
//    ╭─[ misc_dynamic_types_internal_cycle_errors.baml:1:1 ]
//    │
//  1 │ function TypeBuilderFn() -> string {
//    │ │ 
//    │ ╰─ class `DynamicClass` redefined here
//    │ │ 
//    │ ╰─ `DynamicClass` first defined in misc_dynamic_types_internal_cycle_errors.baml
//    │ 
//    │ Note: Error code: E0011
// ───╯
