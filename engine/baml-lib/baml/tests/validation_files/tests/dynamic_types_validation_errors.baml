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
