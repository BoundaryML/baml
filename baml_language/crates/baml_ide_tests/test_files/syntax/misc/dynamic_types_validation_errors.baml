function TypeBuilderFn() -> string {
  client "openai/gpt-4o-mini"
  prompt #"Hello"#
}

class NonDynamic {
  a string
  b string
}

test AttemptToModifyNonDynamicClass {
  functions [TypeBuilderFn]
  type_builder {
    dynamic class NonDynamic {
      c string
    }
  }
args {
    from_text "Test"
  }
}

type SomeAlias = NonDynamic

test AttemptToModifyTypeAlias {
  functions [TypeBuilderFn]
  type_builder {
    dynamic class SomeAlias {
      c string
    }
  }
args {
    from_text "Test"
  }
}

class DynamicClass {
  a string
  b string
  @@dynamic
}

test AttemptToAddDynamicAttrInDyanmicDef {
  functions [TypeBuilderFn]
  type_builder {
    class NotAllowedHere {
      a string
      @@dynamic
    }
    enum StillNotAllowed {
      A
      @@dynamic
    }
    dynamic class DynamicClass {
      c string
      @@dynamic
    }
  }
args {
    from_text "Test"
  }
}

test AttemptToModifySameDynamicMultipleTimes {
  functions [TypeBuilderFn]
  type_builder {
    dynamic class DynamicClass {
      c string
    }
    dynamic class DynamicClass {
      d string
    }
}
args {
    from_text "Test"
  }
}

test NameAlreadyExists {
  functions [TypeBuilderFn]
  type_builder {
    class NonDynamic {
      a string
      b string
    }
    dynamic class DynamicClass {
      non_dynamic NonDynamic
    }
  }
args {
    from_text "Test"
  }
}

enum DynamicEnum {
  A
  B
  @@dynamic
}

test TypeMismatch {
  functions [TypeBuilderFn]
  type_builder {
    dynamic class DynamicEnum {
      C
    }
    dynamic enum DynamicClass {
      c string
    }
}
args {
    from_text "Test"
  }
}

// error: Error validating: Type 'NonDynamic' does not contain the `@@dynamic` attribute so it cannot be modified in a type builder block
//   -->  tests/dynamic_types_validation_errors.baml:14
//    | 
// 13 |   type_builder {
// 14 |     dynamic class NonDynamic {
// 15 |       c string
// 16 |     }
//    | 
// error: Error validating: The `dynamic` keyword only works on classes and enums, but type 'SomeAlias' is a type alias
//   -->  tests/dynamic_types_validation_errors.baml:28
//    | 
// 27 |   type_builder {
// 28 |     dynamic class SomeAlias {
// 29 |       c string
// 30 |     }
//    | 
// error: Error validating: The `@@dynamic` attribute is not allowed in type_builder blocks
//   -->  tests/dynamic_types_validation_errors.baml:46
//    | 
// 45 |   type_builder {
// 46 |     class NotAllowedHere {
// 47 |       a string
// 48 |       @@dynamic
// 49 |     }
//    | 
// error: Error validating: The `@@dynamic` attribute is not allowed in type_builder blocks
//   -->  tests/dynamic_types_validation_errors.baml:50
//    | 
// 49 |     }
// 50 |     enum StillNotAllowed {
// 51 |       A
// 52 |       @@dynamic
// 53 |     }
//    | 
// error: Error validating: Dynamic type definitions cannot contain the `@@dynamic` attribute
//   -->  tests/dynamic_types_validation_errors.baml:54
//    | 
// 53 |     }
// 54 |     dynamic class DynamicClass {
// 55 |       c string
// 56 |       @@dynamic
// 57 |     }
//    | 
// error: The class "DynamicClass" cannot be re-defined because a class with that name already exists.
//   -->  tests/dynamic_types_validation_errors.baml:67
//    | 
// 66 |   type_builder {
// 67 |     dynamic class DynamicClass {
// 68 |       c string
// 69 |     }
//    | 
//   -->  tests/dynamic_types_validation_errors.baml:70
//    | 
// 69 |     }
// 70 |     dynamic class DynamicClass {
// 71 |       d string
// 72 |     }
//    | 
// error: The class "DynamicClass" cannot be re-defined because a class with that name already exists.
//   -->  tests/dynamic_types_validation_errors.baml:70
//    | 
// 69 |     }
// 70 |     dynamic class DynamicClass {
// 71 |       d string
// 72 |     }
//    | 
//   -->  tests/dynamic_types_validation_errors.baml:67
//    | 
// 66 |   type_builder {
// 67 |     dynamic class DynamicClass {
// 68 |       c string
// 69 |     }
//    | 
// error: The class "NonDynamic" cannot be re-defined because a class with that name already exists.
//   -->  tests/dynamic_types_validation_errors.baml:6
//    | 
//  5 | 
//  6 | class NonDynamic {
//    | 
//   -->  tests/dynamic_types_validation_errors.baml:82
//    | 
// 81 |   type_builder {
// 82 |     class NonDynamic {
//    | 
// error: The class "NonDynamic" cannot be re-defined because a class with that name already exists.
//   -->  tests/dynamic_types_validation_errors.baml:82
//    | 
// 81 |   type_builder {
// 82 |     class NonDynamic {
//    | 
//   -->  tests/dynamic_types_validation_errors.baml:6
//    | 
//  5 | 
//  6 | class NonDynamic {
//    | 
// error: Error validating: Type 'DynamicEnum' is an enum, but the dynamic block is defined as 'dynamic class'
//   -->  tests/dynamic_types_validation_errors.baml:104
//    | 
// 103 |   type_builder {
// 104 |     dynamic class DynamicEnum {
// 105 |       C
// 106 |     }
//    | 
// error: Error validating: Type 'DynamicClass' is a class, but the dynamic block is defined as 'dynamic enum'
//   -->  tests/dynamic_types_validation_errors.baml:107
//    | 
// 106 |     }
// 107 |     dynamic enum DynamicClass {
// 108 |       c string
// 109 |     }
//    |

//----
//- diagnostics
// Error: Expected '}', found type_builder
//     ╭─[ misc_dynamic_types_validation_errors.baml:13:3 ]
//     │
//  13 │   type_builder {
//     │   ──────┬─────  
//     │         ╰─────── Expected '}', found type_builder
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found type_builder
//     ╭─[ misc_dynamic_types_validation_errors.baml:13:3 ]
//     │
//  13 │   type_builder {
//     │   ──────┬─────  
//     │         ╰─────── Expected top-level declaration, found type_builder
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found '{'
//     ╭─[ misc_dynamic_types_validation_errors.baml:13:16 ]
//     │
//  13 │   type_builder {
//     │                ┬  
//     │                ╰── Expected top-level declaration, found '{'
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found dynamic
//     ╭─[ misc_dynamic_types_validation_errors.baml:14:5 ]
//     │
//  14 │     dynamic class NonDynamic {
//     │     ───┬───  
//     │        ╰───── Expected top-level declaration, found dynamic
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found '}'
//     ╭─[ misc_dynamic_types_validation_errors.baml:17:3 ]
//     │
//  17 │   }
//     │   ┬  
//     │   ╰── Expected top-level declaration, found '}'
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found identifier
//     ╭─[ misc_dynamic_types_validation_errors.baml:18:1 ]
//     │
//  18 │ args {
//     │ ──┬─  
//     │   ╰─── Expected top-level declaration, found identifier
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found '{'
//     ╭─[ misc_dynamic_types_validation_errors.baml:18:6 ]
//     │
//  18 │ args {
//     │      ┬  
//     │      ╰── Expected top-level declaration, found '{'
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found identifier
//     ╭─[ misc_dynamic_types_validation_errors.baml:19:5 ]
//     │
//  19 │     from_text "Test"
//     │     ────┬────  
//     │         ╰────── Expected top-level declaration, found identifier
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found '"'
//     ╭─[ misc_dynamic_types_validation_errors.baml:19:15 ]
//     │
//  19 │     from_text "Test"
//     │               ┬  
//     │               ╰── Expected top-level declaration, found '"'
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found identifier
//     ╭─[ misc_dynamic_types_validation_errors.baml:19:16 ]
//     │
//  19 │     from_text "Test"
//     │                ──┬─  
//     │                  ╰─── Expected top-level declaration, found identifier
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found '"'
//     ╭─[ misc_dynamic_types_validation_errors.baml:19:20 ]
//     │
//  19 │     from_text "Test"
//     │                    ┬  
//     │                    ╰── Expected top-level declaration, found '"'
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found '}'
//     ╭─[ misc_dynamic_types_validation_errors.baml:20:3 ]
//     │
//  20 │   }
//     │   ┬  
//     │   ╰── Expected top-level declaration, found '}'
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found '}'
//     ╭─[ misc_dynamic_types_validation_errors.baml:21:1 ]
//     │
//  21 │ }
//     │ ┬  
//     │ ╰── Expected top-level declaration, found '}'
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected '}', found type_builder
//     ╭─[ misc_dynamic_types_validation_errors.baml:27:3 ]
//     │
//  27 │   type_builder {
//     │   ──────┬─────  
//     │         ╰─────── Expected '}', found type_builder
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found type_builder
//     ╭─[ misc_dynamic_types_validation_errors.baml:27:3 ]
//     │
//  27 │   type_builder {
//     │   ──────┬─────  
//     │         ╰─────── Expected top-level declaration, found type_builder
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found '{'
//     ╭─[ misc_dynamic_types_validation_errors.baml:27:16 ]
//     │
//  27 │   type_builder {
//     │                ┬  
//     │                ╰── Expected top-level declaration, found '{'
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found dynamic
//     ╭─[ misc_dynamic_types_validation_errors.baml:28:5 ]
//     │
//  28 │     dynamic class SomeAlias {
//     │     ───┬───  
//     │        ╰───── Expected top-level declaration, found dynamic
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found '}'
//     ╭─[ misc_dynamic_types_validation_errors.baml:31:3 ]
//     │
//  31 │   }
//     │   ┬  
//     │   ╰── Expected top-level declaration, found '}'
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found identifier
//     ╭─[ misc_dynamic_types_validation_errors.baml:32:1 ]
//     │
//  32 │ args {
//     │ ──┬─  
//     │   ╰─── Expected top-level declaration, found identifier
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found '{'
//     ╭─[ misc_dynamic_types_validation_errors.baml:32:6 ]
//     │
//  32 │ args {
//     │      ┬  
//     │      ╰── Expected top-level declaration, found '{'
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found identifier
//     ╭─[ misc_dynamic_types_validation_errors.baml:33:5 ]
//     │
//  33 │     from_text "Test"
//     │     ────┬────  
//     │         ╰────── Expected top-level declaration, found identifier
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found '"'
//     ╭─[ misc_dynamic_types_validation_errors.baml:33:15 ]
//     │
//  33 │     from_text "Test"
//     │               ┬  
//     │               ╰── Expected top-level declaration, found '"'
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found identifier
//     ╭─[ misc_dynamic_types_validation_errors.baml:33:16 ]
//     │
//  33 │     from_text "Test"
//     │                ──┬─  
//     │                  ╰─── Expected top-level declaration, found identifier
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found '"'
//     ╭─[ misc_dynamic_types_validation_errors.baml:33:20 ]
//     │
//  33 │     from_text "Test"
//     │                    ┬  
//     │                    ╰── Expected top-level declaration, found '"'
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found '}'
//     ╭─[ misc_dynamic_types_validation_errors.baml:34:3 ]
//     │
//  34 │   }
//     │   ┬  
//     │   ╰── Expected top-level declaration, found '}'
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found '}'
//     ╭─[ misc_dynamic_types_validation_errors.baml:35:1 ]
//     │
//  35 │ }
//     │ ┬  
//     │ ╰── Expected top-level declaration, found '}'
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected '}', found type_builder
//     ╭─[ misc_dynamic_types_validation_errors.baml:45:3 ]
//     │
//  45 │   type_builder {
//     │   ──────┬─────  
//     │         ╰─────── Expected '}', found type_builder
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found type_builder
//     ╭─[ misc_dynamic_types_validation_errors.baml:45:3 ]
//     │
//  45 │   type_builder {
//     │   ──────┬─────  
//     │         ╰─────── Expected top-level declaration, found type_builder
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found '{'
//     ╭─[ misc_dynamic_types_validation_errors.baml:45:16 ]
//     │
//  45 │   type_builder {
//     │                ┬  
//     │                ╰── Expected top-level declaration, found '{'
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found dynamic
//     ╭─[ misc_dynamic_types_validation_errors.baml:54:5 ]
//     │
//  54 │     dynamic class DynamicClass {
//     │     ───┬───  
//     │        ╰───── Expected top-level declaration, found dynamic
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found '}'
//     ╭─[ misc_dynamic_types_validation_errors.baml:58:3 ]
//     │
//  58 │   }
//     │   ┬  
//     │   ╰── Expected top-level declaration, found '}'
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found identifier
//     ╭─[ misc_dynamic_types_validation_errors.baml:59:1 ]
//     │
//  59 │ args {
//     │ ──┬─  
//     │   ╰─── Expected top-level declaration, found identifier
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found '{'
//     ╭─[ misc_dynamic_types_validation_errors.baml:59:6 ]
//     │
//  59 │ args {
//     │      ┬  
//     │      ╰── Expected top-level declaration, found '{'
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found identifier
//     ╭─[ misc_dynamic_types_validation_errors.baml:60:5 ]
//     │
//  60 │     from_text "Test"
//     │     ────┬────  
//     │         ╰────── Expected top-level declaration, found identifier
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found '"'
//     ╭─[ misc_dynamic_types_validation_errors.baml:60:15 ]
//     │
//  60 │     from_text "Test"
//     │               ┬  
//     │               ╰── Expected top-level declaration, found '"'
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found identifier
//     ╭─[ misc_dynamic_types_validation_errors.baml:60:16 ]
//     │
//  60 │     from_text "Test"
//     │                ──┬─  
//     │                  ╰─── Expected top-level declaration, found identifier
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found '"'
//     ╭─[ misc_dynamic_types_validation_errors.baml:60:20 ]
//     │
//  60 │     from_text "Test"
//     │                    ┬  
//     │                    ╰── Expected top-level declaration, found '"'
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found '}'
//     ╭─[ misc_dynamic_types_validation_errors.baml:61:3 ]
//     │
//  61 │   }
//     │   ┬  
//     │   ╰── Expected top-level declaration, found '}'
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found '}'
//     ╭─[ misc_dynamic_types_validation_errors.baml:62:1 ]
//     │
//  62 │ }
//     │ ┬  
//     │ ╰── Expected top-level declaration, found '}'
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected '}', found type_builder
//     ╭─[ misc_dynamic_types_validation_errors.baml:66:3 ]
//     │
//  66 │   type_builder {
//     │   ──────┬─────  
//     │         ╰─────── Expected '}', found type_builder
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found type_builder
//     ╭─[ misc_dynamic_types_validation_errors.baml:66:3 ]
//     │
//  66 │   type_builder {
//     │   ──────┬─────  
//     │         ╰─────── Expected top-level declaration, found type_builder
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found '{'
//     ╭─[ misc_dynamic_types_validation_errors.baml:66:16 ]
//     │
//  66 │   type_builder {
//     │                ┬  
//     │                ╰── Expected top-level declaration, found '{'
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found dynamic
//     ╭─[ misc_dynamic_types_validation_errors.baml:67:5 ]
//     │
//  67 │     dynamic class DynamicClass {
//     │     ───┬───  
//     │        ╰───── Expected top-level declaration, found dynamic
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found dynamic
//     ╭─[ misc_dynamic_types_validation_errors.baml:70:5 ]
//     │
//  70 │     dynamic class DynamicClass {
//     │     ───┬───  
//     │        ╰───── Expected top-level declaration, found dynamic
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found '}'
//     ╭─[ misc_dynamic_types_validation_errors.baml:73:1 ]
//     │
//  73 │ }
//     │ ┬  
//     │ ╰── Expected top-level declaration, found '}'
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found identifier
//     ╭─[ misc_dynamic_types_validation_errors.baml:74:1 ]
//     │
//  74 │ args {
//     │ ──┬─  
//     │   ╰─── Expected top-level declaration, found identifier
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found '{'
//     ╭─[ misc_dynamic_types_validation_errors.baml:74:6 ]
//     │
//  74 │ args {
//     │      ┬  
//     │      ╰── Expected top-level declaration, found '{'
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found identifier
//     ╭─[ misc_dynamic_types_validation_errors.baml:75:5 ]
//     │
//  75 │     from_text "Test"
//     │     ────┬────  
//     │         ╰────── Expected top-level declaration, found identifier
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found '"'
//     ╭─[ misc_dynamic_types_validation_errors.baml:75:15 ]
//     │
//  75 │     from_text "Test"
//     │               ┬  
//     │               ╰── Expected top-level declaration, found '"'
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found identifier
//     ╭─[ misc_dynamic_types_validation_errors.baml:75:16 ]
//     │
//  75 │     from_text "Test"
//     │                ──┬─  
//     │                  ╰─── Expected top-level declaration, found identifier
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found '"'
//     ╭─[ misc_dynamic_types_validation_errors.baml:75:20 ]
//     │
//  75 │     from_text "Test"
//     │                    ┬  
//     │                    ╰── Expected top-level declaration, found '"'
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found '}'
//     ╭─[ misc_dynamic_types_validation_errors.baml:76:3 ]
//     │
//  76 │   }
//     │   ┬  
//     │   ╰── Expected top-level declaration, found '}'
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found '}'
//     ╭─[ misc_dynamic_types_validation_errors.baml:77:1 ]
//     │
//  77 │ }
//     │ ┬  
//     │ ╰── Expected top-level declaration, found '}'
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected '}', found type_builder
//     ╭─[ misc_dynamic_types_validation_errors.baml:81:3 ]
//     │
//  81 │   type_builder {
//     │   ──────┬─────  
//     │         ╰─────── Expected '}', found type_builder
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found type_builder
//     ╭─[ misc_dynamic_types_validation_errors.baml:81:3 ]
//     │
//  81 │   type_builder {
//     │   ──────┬─────  
//     │         ╰─────── Expected top-level declaration, found type_builder
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found '{'
//     ╭─[ misc_dynamic_types_validation_errors.baml:81:16 ]
//     │
//  81 │   type_builder {
//     │                ┬  
//     │                ╰── Expected top-level declaration, found '{'
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found dynamic
//     ╭─[ misc_dynamic_types_validation_errors.baml:86:5 ]
//     │
//  86 │     dynamic class DynamicClass {
//     │     ───┬───  
//     │        ╰───── Expected top-level declaration, found dynamic
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found '}'
//     ╭─[ misc_dynamic_types_validation_errors.baml:89:3 ]
//     │
//  89 │   }
//     │   ┬  
//     │   ╰── Expected top-level declaration, found '}'
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found identifier
//     ╭─[ misc_dynamic_types_validation_errors.baml:90:1 ]
//     │
//  90 │ args {
//     │ ──┬─  
//     │   ╰─── Expected top-level declaration, found identifier
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found '{'
//     ╭─[ misc_dynamic_types_validation_errors.baml:90:6 ]
//     │
//  90 │ args {
//     │      ┬  
//     │      ╰── Expected top-level declaration, found '{'
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found identifier
//     ╭─[ misc_dynamic_types_validation_errors.baml:91:5 ]
//     │
//  91 │     from_text "Test"
//     │     ────┬────  
//     │         ╰────── Expected top-level declaration, found identifier
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found '"'
//     ╭─[ misc_dynamic_types_validation_errors.baml:91:15 ]
//     │
//  91 │     from_text "Test"
//     │               ┬  
//     │               ╰── Expected top-level declaration, found '"'
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found identifier
//     ╭─[ misc_dynamic_types_validation_errors.baml:91:16 ]
//     │
//  91 │     from_text "Test"
//     │                ──┬─  
//     │                  ╰─── Expected top-level declaration, found identifier
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found '"'
//     ╭─[ misc_dynamic_types_validation_errors.baml:91:20 ]
//     │
//  91 │     from_text "Test"
//     │                    ┬  
//     │                    ╰── Expected top-level declaration, found '"'
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found '}'
//     ╭─[ misc_dynamic_types_validation_errors.baml:92:3 ]
//     │
//  92 │   }
//     │   ┬  
//     │   ╰── Expected top-level declaration, found '}'
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found '}'
//     ╭─[ misc_dynamic_types_validation_errors.baml:93:1 ]
//     │
//  93 │ }
//     │ ┬  
//     │ ╰── Expected top-level declaration, found '}'
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected '}', found type_builder
//      ╭─[ misc_dynamic_types_validation_errors.baml:103:3 ]
//      │
//  103 │   type_builder {
//      │   ──────┬─────  
//      │         ╰─────── Expected '}', found type_builder
//      │ 
//      │ Note: Error code: E0010
// ─────╯
// Error: Expected top-level declaration, found type_builder
//      ╭─[ misc_dynamic_types_validation_errors.baml:103:3 ]
//      │
//  103 │   type_builder {
//      │   ──────┬─────  
//      │         ╰─────── Expected top-level declaration, found type_builder
//      │ 
//      │ Note: Error code: E0010
// ─────╯
// Error: Expected top-level declaration, found '{'
//      ╭─[ misc_dynamic_types_validation_errors.baml:103:16 ]
//      │
//  103 │   type_builder {
//      │                ┬  
//      │                ╰── Expected top-level declaration, found '{'
//      │ 
//      │ Note: Error code: E0010
// ─────╯
// Error: Expected top-level declaration, found dynamic
//      ╭─[ misc_dynamic_types_validation_errors.baml:104:5 ]
//      │
//  104 │     dynamic class DynamicEnum {
//      │     ───┬───  
//      │        ╰───── Expected top-level declaration, found dynamic
//      │ 
//      │ Note: Error code: E0010
// ─────╯
// Error: field 'C' is missing a type annotation
//      ╭─[ misc_dynamic_types_validation_errors.baml:105:7 ]
//      │
//  105 │       C
//      │       ┬  
//      │       ╰── field 'C' is missing a type annotation
//      │ 
//      │ Note: Error code: E0010
// ─────╯
// Error: Expected top-level declaration, found dynamic
//      ╭─[ misc_dynamic_types_validation_errors.baml:107:5 ]
//      │
//  107 │     dynamic enum DynamicClass {
//      │     ───┬───  
//      │        ╰───── Expected top-level declaration, found dynamic
//      │ 
//      │ Note: Error code: E0010
// ─────╯
// Error: Expected top-level declaration, found '}'
//      ╭─[ misc_dynamic_types_validation_errors.baml:110:1 ]
//      │
//  110 │ }
//      │ ┬  
//      │ ╰── Expected top-level declaration, found '}'
//      │ 
//      │ Note: Error code: E0010
// ─────╯
// Error: Expected top-level declaration, found identifier
//      ╭─[ misc_dynamic_types_validation_errors.baml:111:1 ]
//      │
//  111 │ args {
//      │ ──┬─  
//      │   ╰─── Expected top-level declaration, found identifier
//      │ 
//      │ Note: Error code: E0010
// ─────╯
// Error: Expected top-level declaration, found '{'
//      ╭─[ misc_dynamic_types_validation_errors.baml:111:6 ]
//      │
//  111 │ args {
//      │      ┬  
//      │      ╰── Expected top-level declaration, found '{'
//      │ 
//      │ Note: Error code: E0010
// ─────╯
// Error: Expected top-level declaration, found identifier
//      ╭─[ misc_dynamic_types_validation_errors.baml:112:5 ]
//      │
//  112 │     from_text "Test"
//      │     ────┬────  
//      │         ╰────── Expected top-level declaration, found identifier
//      │ 
//      │ Note: Error code: E0010
// ─────╯
// Error: Expected top-level declaration, found '"'
//      ╭─[ misc_dynamic_types_validation_errors.baml:112:15 ]
//      │
//  112 │     from_text "Test"
//      │               ┬  
//      │               ╰── Expected top-level declaration, found '"'
//      │ 
//      │ Note: Error code: E0010
// ─────╯
// Error: Expected top-level declaration, found identifier
//      ╭─[ misc_dynamic_types_validation_errors.baml:112:16 ]
//      │
//  112 │     from_text "Test"
//      │                ──┬─  
//      │                  ╰─── Expected top-level declaration, found identifier
//      │ 
//      │ Note: Error code: E0010
// ─────╯
// Error: Expected top-level declaration, found '"'
//      ╭─[ misc_dynamic_types_validation_errors.baml:112:20 ]
//      │
//  112 │     from_text "Test"
//      │                    ┬  
//      │                    ╰── Expected top-level declaration, found '"'
//      │ 
//      │ Note: Error code: E0010
// ─────╯
// Error: Expected top-level declaration, found '}'
//      ╭─[ misc_dynamic_types_validation_errors.baml:113:3 ]
//      │
//  113 │   }
//      │   ┬  
//      │   ╰── Expected top-level declaration, found '}'
//      │ 
//      │ Note: Error code: E0010
// ─────╯
// Error: Expected top-level declaration, found '}'
//      ╭─[ misc_dynamic_types_validation_errors.baml:114:1 ]
//      │
//  114 │ }
//      │ ┬  
//      │ ╰── Expected top-level declaration, found '}'
//      │ 
//      │ Note: Error code: E0010
// ─────╯
// Error: Duplicate class `NonDynamic`
//    ╭─[ misc_dynamic_types_validation_errors.baml:1:1 ]
//    │
//  1 │ function TypeBuilderFn() -> string {
//    │ │ 
//    │ ╰─ class `NonDynamic` redefined here
//    │ │ 
//    │ ╰─ `NonDynamic` first defined in misc_dynamic_types_validation_errors.baml
//    │ 
//    │ Note: Error code: E0011
// ───╯
// Error: Duplicate class `NonDynamic`
//    ╭─[ misc_dynamic_types_validation_errors.baml:1:1 ]
//    │
//  1 │ function TypeBuilderFn() -> string {
//    │ │ 
//    │ ╰─ class `NonDynamic` redefined here
//    │ │ 
//    │ ╰─ `NonDynamic` first defined in misc_dynamic_types_validation_errors.baml
//    │ 
//    │ Note: Error code: E0011
// ───╯
// Error: Duplicate class `DynamicClass`
//    ╭─[ misc_dynamic_types_validation_errors.baml:1:1 ]
//    │
//  1 │ function TypeBuilderFn() -> string {
//    │ │ 
//    │ ╰─ class `DynamicClass` redefined here
//    │ │ 
//    │ ╰─ `DynamicClass` first defined in misc_dynamic_types_validation_errors.baml
//    │ 
//    │ Note: Error code: E0011
// ───╯
// Error: Duplicate class `DynamicClass`
//    ╭─[ misc_dynamic_types_validation_errors.baml:1:1 ]
//    │
//  1 │ function TypeBuilderFn() -> string {
//    │ │ 
//    │ ╰─ class `DynamicClass` redefined here
//    │ │ 
//    │ ╰─ `DynamicClass` first defined in misc_dynamic_types_validation_errors.baml
//    │ 
//    │ Note: Error code: E0011
// ───╯
// Error: Duplicate class `DynamicClass`
//    ╭─[ misc_dynamic_types_validation_errors.baml:1:1 ]
//    │
//  1 │ function TypeBuilderFn() -> string {
//    │ │ 
//    │ ╰─ class `DynamicClass` redefined here
//    │ │ 
//    │ ╰─ `DynamicClass` first defined in misc_dynamic_types_validation_errors.baml
//    │ 
//    │ Note: Error code: E0011
// ───╯
// Error: Duplicate class `DynamicClass`
//    ╭─[ misc_dynamic_types_validation_errors.baml:1:1 ]
//    │
//  1 │ function TypeBuilderFn() -> string {
//    │ │ 
//    │ ╰─ class `DynamicClass` redefined here
//    │ │ 
//    │ ╰─ `DynamicClass` first defined in misc_dynamic_types_validation_errors.baml
//    │ 
//    │ Note: Error code: E0011
// ───╯
// Error: Duplicate enum `DynamicEnum`
//    ╭─[ misc_dynamic_types_validation_errors.baml:1:1 ]
//    │
//  1 │ function TypeBuilderFn() -> string {
//    │ │ 
//    │ ╰─ enum `DynamicEnum` redefined here
//    │ │ 
//    │ ╰─ `DynamicEnum` first defined in misc_dynamic_types_validation_errors.baml
//    │ 
//    │ Note: Error code: E0011
// ───╯
// Error: Duplicate enum `DynamicClass`
//    ╭─[ misc_dynamic_types_validation_errors.baml:1:1 ]
//    │
//  1 │ function TypeBuilderFn() -> string {
//    │ │ 
//    │ ╰─ enum `DynamicClass` redefined here
//    │ │ 
//    │ ╰─ `DynamicClass` first defined in misc_dynamic_types_validation_errors.baml
//    │ 
//    │ Note: Error code: E0011
// ───╯
// Error: Duplicate type alias `SomeAlias`
//    ╭─[ misc_dynamic_types_validation_errors.baml:1:1 ]
//    │
//  1 │ function TypeBuilderFn() -> string {
//    │ │ 
//    │ ╰─ type alias `SomeAlias` redefined here
//    │ │ 
//    │ ╰─ `SomeAlias` first defined in misc_dynamic_types_validation_errors.baml
//    │ 
//    │ Note: Error code: E0011
// ───╯
