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
    dynamic NonDynamic {
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
    dynamic SomeAlias {
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
    dynamic DynamicClass {
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
    dynamic DynamicClass {
      c string
    }
    dynamic DynamicClass {
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
    dynamic DynamicClass {
      non_dynamic NonDynamic
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
// 14 |     dynamic NonDynamic {
// 15 |       c string
// 16 |     }
//    | 
// error: Error validating: The `dynamic` keyword only works on classes and enums, but type 'SomeAlias' is a type alias
//   -->  tests/dynamic_types_validation_errors.baml:28
//    | 
// 27 |   type_builder {
// 28 |     dynamic SomeAlias {
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
// 54 |     dynamic DynamicClass {
// 55 |       c string
// 56 |       @@dynamic
// 57 |     }
//    | 
// error: Error validating: Multiple dynamic definitions for type `DynamicClass`
//   -->  tests/dynamic_types_validation_errors.baml:70
//    | 
// 69 |     }
// 70 |     dynamic DynamicClass {
// 71 |       d string
// 72 |     }
//    | 
// error: The class "NonDynamic" cannot be defined because a class with that name already exists.
//   -->  tests/dynamic_types_validation_errors.baml:82
//    | 
// 81 |   type_builder {
// 82 |     class NonDynamic {
//    | 
