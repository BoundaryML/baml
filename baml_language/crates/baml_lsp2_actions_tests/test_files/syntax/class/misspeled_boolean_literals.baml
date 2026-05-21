class TestLiterals {
  a "SingleLiteral"
  b "boolean" | True | False
}

// error: Type `True` does not exist. Did you mean one of these: `true`, `string`, `int`, `bool`, `false`, `float`, `TestLiterals`?
//   -->  class/misspeled_boolean_literals.baml:3
//    |
//  2 |   a "SingleLiteral"
//  3 |   b "boolean" | True | False
//    |
// error: Type `False` does not exist. Did you mean one of these: `false`, `float`, `true`, `int`, `bool`, `string`, `TestLiterals`?
//   -->  class/misspeled_boolean_literals.baml:3
//    |
//  2 |   a "SingleLiteral"
//  3 |   b "boolean" | True | False
//    |

//----
//- diagnostics
// error: unresolved type: True
//    ╭─[ class_misspeled_boolean_literals.baml:3:4 ]
//    │
//  3 │   b "boolean" | True | False
//    │    ────────────┬────────────  
//    │                ╰────────────── unresolved type: True
//    │ 
//    │ Note: Error code: E0002
// ───╯
// error: unresolved type: False
//    ╭─[ class_misspeled_boolean_literals.baml:3:4 ]
//    │
//  3 │   b "boolean" | True | False
//    │    ────────────┬────────────  
//    │                ╰────────────── unresolved type: False
//    │ 
//    │ Note: Error code: E0002
// ───╯
