function TypeBuilderFn() -> string { 
  client "openai/gpt-4o-mini"
  prompt #"Hello"#
}

class DynamicClass {
  a string
  b string
  @@dynamic
}

test AttemptToIntroduceInfiniteCycle {
  functions [TypeBuilderFn]
  type_builder {
    class A {
      p B
    }
    class B {
      p C
    }
    class C {
      p A
    }

    dynamic class DynamicClass {
      cycle A
    }
  }
  args {
    from_text "Test"
  }
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

// error: Error validating: These classes form a dependency cycle: A -> B -> C
//   -->  tests/dynamic_types_external_cycle_errors.baml:15
//    | 
// 14 |   type_builder {
// 15 |     class A {
// 16 |       p B
// 17 |     }
//    |

//----
//- diagnostics
// Error: Expected '}', found type_builder
//     ╭─[ misc_dynamic_types_external_cycle_errors.baml:14:3 ]
//     │
//  14 │   type_builder {
//     │   ──────┬─────  
//     │         ╰─────── Expected '}', found type_builder
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found type_builder
//     ╭─[ misc_dynamic_types_external_cycle_errors.baml:14:3 ]
//     │
//  14 │   type_builder {
//     │   ──────┬─────  
//     │         ╰─────── Expected top-level declaration, found type_builder
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found '{'
//     ╭─[ misc_dynamic_types_external_cycle_errors.baml:14:16 ]
//     │
//  14 │   type_builder {
//     │                ┬  
//     │                ╰── Expected top-level declaration, found '{'
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found dynamic
//     ╭─[ misc_dynamic_types_external_cycle_errors.baml:25:5 ]
//     │
//  25 │     dynamic class DynamicClass {
//     │     ───┬───  
//     │        ╰───── Expected top-level declaration, found dynamic
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found '}'
//     ╭─[ misc_dynamic_types_external_cycle_errors.baml:28:3 ]
//     │
//  28 │   }
//     │   ┬  
//     │   ╰── Expected top-level declaration, found '}'
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found identifier
//     ╭─[ misc_dynamic_types_external_cycle_errors.baml:29:3 ]
//     │
//  29 │   args {
//     │   ──┬─  
//     │     ╰─── Expected top-level declaration, found identifier
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found '{'
//     ╭─[ misc_dynamic_types_external_cycle_errors.baml:29:8 ]
//     │
//  29 │   args {
//     │        ┬  
//     │        ╰── Expected top-level declaration, found '{'
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found identifier
//     ╭─[ misc_dynamic_types_external_cycle_errors.baml:30:5 ]
//     │
//  30 │     from_text "Test"
//     │     ────┬────  
//     │         ╰────── Expected top-level declaration, found identifier
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found '"'
//     ╭─[ misc_dynamic_types_external_cycle_errors.baml:30:15 ]
//     │
//  30 │     from_text "Test"
//     │               ┬  
//     │               ╰── Expected top-level declaration, found '"'
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found identifier
//     ╭─[ misc_dynamic_types_external_cycle_errors.baml:30:16 ]
//     │
//  30 │     from_text "Test"
//     │                ──┬─  
//     │                  ╰─── Expected top-level declaration, found identifier
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found '"'
//     ╭─[ misc_dynamic_types_external_cycle_errors.baml:30:20 ]
//     │
//  30 │     from_text "Test"
//     │                    ┬  
//     │                    ╰── Expected top-level declaration, found '"'
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found '}'
//     ╭─[ misc_dynamic_types_external_cycle_errors.baml:31:3 ]
//     │
//  31 │   }
//     │   ┬  
//     │   ╰── Expected top-level declaration, found '}'
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found '}'
//     ╭─[ misc_dynamic_types_external_cycle_errors.baml:32:1 ]
//     │
//  32 │ }
//     │ ┬  
//     │ ╰── Expected top-level declaration, found '}'
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected '}', found type_builder
//     ╭─[ misc_dynamic_types_external_cycle_errors.baml:36:3 ]
//     │
//  36 │   type_builder {
//     │   ──────┬─────  
//     │         ╰─────── Expected '}', found type_builder
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found type_builder
//     ╭─[ misc_dynamic_types_external_cycle_errors.baml:36:3 ]
//     │
//  36 │   type_builder {
//     │   ──────┬─────  
//     │         ╰─────── Expected top-level declaration, found type_builder
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found '{'
//     ╭─[ misc_dynamic_types_external_cycle_errors.baml:36:16 ]
//     │
//  36 │   type_builder {
//     │                ┬  
//     │                ╰── Expected top-level declaration, found '{'
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found dynamic
//     ╭─[ misc_dynamic_types_external_cycle_errors.baml:37:5 ]
//     │
//  37 │     dynamic class DynamicClass {
//     │     ───┬───  
//     │        ╰───── Expected top-level declaration, found dynamic
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found '}'
//     ╭─[ misc_dynamic_types_external_cycle_errors.baml:40:3 ]
//     │
//  40 │   }
//     │   ┬  
//     │   ╰── Expected top-level declaration, found '}'
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found identifier
//     ╭─[ misc_dynamic_types_external_cycle_errors.baml:41:3 ]
//     │
//  41 │   args {
//     │   ──┬─  
//     │     ╰─── Expected top-level declaration, found identifier
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found '{'
//     ╭─[ misc_dynamic_types_external_cycle_errors.baml:41:8 ]
//     │
//  41 │   args {
//     │        ┬  
//     │        ╰── Expected top-level declaration, found '{'
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found identifier
//     ╭─[ misc_dynamic_types_external_cycle_errors.baml:42:5 ]
//     │
//  42 │     from_text "Test"
//     │     ────┬────  
//     │         ╰────── Expected top-level declaration, found identifier
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found '"'
//     ╭─[ misc_dynamic_types_external_cycle_errors.baml:42:15 ]
//     │
//  42 │     from_text "Test"
//     │               ┬  
//     │               ╰── Expected top-level declaration, found '"'
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found identifier
//     ╭─[ misc_dynamic_types_external_cycle_errors.baml:42:16 ]
//     │
//  42 │     from_text "Test"
//     │                ──┬─  
//     │                  ╰─── Expected top-level declaration, found identifier
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found '"'
//     ╭─[ misc_dynamic_types_external_cycle_errors.baml:42:20 ]
//     │
//  42 │     from_text "Test"
//     │                    ┬  
//     │                    ╰── Expected top-level declaration, found '"'
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found '}'
//     ╭─[ misc_dynamic_types_external_cycle_errors.baml:43:3 ]
//     │
//  43 │   }
//     │   ┬  
//     │   ╰── Expected top-level declaration, found '}'
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found '}'
//     ╭─[ misc_dynamic_types_external_cycle_errors.baml:44:1 ]
//     │
//  44 │ }
//     │ ┬  
//     │ ╰── Expected top-level declaration, found '}'
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Duplicate class 'DynamicClass'
//    ╭─[ misc_dynamic_types_external_cycle_errors.baml:1:1 ]
//    │
//  1 │ function TypeBuilderFn() -> string {
//    │ │ 
//    │ ╰─ class 'DynamicClass' defined in misc_dynamic_types_external_cycle_errors.baml
//    │ │ 
//    │ ╰─ 'DynamicClass' previously defined in misc_dynamic_types_external_cycle_errors.baml
//    │ 
//    │ Note: Error code: E0011
// ───╯
// Error: Duplicate class 'DynamicClass'
//    ╭─[ misc_dynamic_types_external_cycle_errors.baml:1:1 ]
//    │
//  1 │ function TypeBuilderFn() -> string {
//    │ │ 
//    │ ╰─ class 'DynamicClass' defined in misc_dynamic_types_external_cycle_errors.baml
//    │ │ 
//    │ ╰─ 'DynamicClass' previously defined in misc_dynamic_types_external_cycle_errors.baml
//    │ 
//    │ Note: Error code: E0011
// ───╯
