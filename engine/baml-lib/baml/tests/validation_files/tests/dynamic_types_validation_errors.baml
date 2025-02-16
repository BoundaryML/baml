function TypeBuilderFn() -> string { 
  client "openai/gpt-4o-mini"
  prompt #"Hello"#
}

class NonDynamic {
  a string
  b string
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

// error: Error validating: The `dynamic` keyword only works on classes and enums, but type 'SomeAlias' is a type alias
//   -->  tests/dynamic_types_validation_errors.baml:16
//    | 
// 15 |   type_builder {
// 16 |     dynamic SomeAlias {
//    | 
// error: The `@@dynamic` attribute is not allowed in type_builder blocks.
//   -->  tests/dynamic_types_validation_errors.baml:36
//    | 
// 35 |       a string
// 36 |       @@dynamic
//    | 
// error: The `@@dynamic` attribute is not allowed in type_builder blocks.
//   -->  tests/dynamic_types_validation_errors.baml:40
//    | 
// 39 |       A
// 40 |       @@dynamic
//    | 
// error: The `@@dynamic` attribute is not allowed in type_builder blocks.
//   -->  tests/dynamic_types_validation_errors.baml:44
//    | 
// 43 |       c string
// 44 |       @@dynamic
//    | 
// error: The class "DynamicClass" cannot be re-defined because a class with that name already exists.
//   -->  tests/dynamic_types_validation_errors.baml:55
//    | 
// 54 |   type_builder {
// 55 |     dynamic DynamicClass {
// 56 |       c string
// 57 |     }
//    | 
//   -->  tests/dynamic_types_validation_errors.baml:58
//    | 
// 57 |     }
// 58 |     dynamic DynamicClass {
// 59 |       d string
// 60 |     }
//    | 
// error: The class "DynamicClass" cannot be re-defined because a class with that name already exists.
//   -->  tests/dynamic_types_validation_errors.baml:58
//    | 
// 57 |     }
// 58 |     dynamic DynamicClass {
// 59 |       d string
// 60 |     }
//    | 
//   -->  tests/dynamic_types_validation_errors.baml:55
//    | 
// 54 |   type_builder {
// 55 |     dynamic DynamicClass {
// 56 |       c string
// 57 |     }
//    | 
// error: The class "NonDynamic" cannot be re-defined because a class with that name already exists.
//   -->  tests/dynamic_types_validation_errors.baml:6
//    | 
//  5 | 
//  6 | class NonDynamic {
//    | 
//   -->  tests/dynamic_types_validation_errors.baml:70
//    | 
// 69 |   type_builder {
// 70 |     class NonDynamic {
//    | 
// error: The class "NonDynamic" cannot be re-defined because a class with that name already exists.
//   -->  tests/dynamic_types_validation_errors.baml:70
//    | 
// 69 |   type_builder {
// 70 |     class NonDynamic {
//    | 
//   -->  tests/dynamic_types_validation_errors.baml:6
//    | 
//  5 | 
//  6 | class NonDynamic {
//    | 
