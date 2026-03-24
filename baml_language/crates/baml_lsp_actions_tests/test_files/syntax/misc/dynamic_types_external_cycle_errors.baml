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
// Error: unreachable arm
//      ╭─[ llm_types.baml:159:17 ]
//      │
//  159 │             _ => { continue; }
//      │                 ───────┬──────  
//      │                        ╰──────── unreachable arm
//      │ 
//      │ Note: Error code: E0001
// ─────╯
// Error: unreachable arm
//      ╭─[ llm_types.baml:189:17 ]
//      │
//  189 │ ╭─▶             _ => {
//      ┆ ┆   
//  194 │ ├─▶             }
//      │ │                   
//      │ ╰─────────────────── unreachable arm
//      │     
//      │     Note: Error code: E0001
// ─────╯
// Error: unreachable arm
//      ╭─[ llm_types.baml:208:17 ]
//      │
//  208 │             _ => { continue; }
//      │                 ───────┬──────  
//      │                        ╰──────── unreachable arm
//      │ 
//      │ Note: Error code: E0001
// ─────╯
