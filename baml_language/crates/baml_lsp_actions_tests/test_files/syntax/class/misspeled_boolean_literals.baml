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
// Error: Unknown type `True`
//    ╭─[ class_misspeled_boolean_literals.baml:3:4 ]
//    │
//  3 │   b "boolean" | True | False
//    │    ────────────┬────────────  
//    │                ╰────────────── Unknown type `True`
//    │ 
//    │ Note: Error code: E0002
// ───╯
// Error: Unknown type `False`
//    ╭─[ class_misspeled_boolean_literals.baml:3:4 ]
//    │
//  3 │   b "boolean" | True | False
//    │    ────────────┬────────────  
//    │                ╰────────────── Unknown type `False`
//    │ 
//    │ Note: Error code: E0002
// ───╯
