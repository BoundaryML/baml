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
// E0002
//
//   × unresolved type: True
//    ╭─[class_misspeled_boolean_literals.baml:3:17]
//  3 │   b "boolean" | True | False
//    ·                 ────
//    ╰────
// E0002
//
//   × unresolved type: False
//    ╭─[class_misspeled_boolean_literals.baml:3:24]
//  3 │   b "boolean" | True | False
//    ·                        ─────
//    ╰────
// E0002
//
//   × unresolved type: True
//    ╭─[class_misspeled_boolean_literals.baml:1:1]
//  1 │ class TestLiterals {
//    · ▲
//    ╰────
// E0002
//
//   × unresolved type: False
//    ╭─[class_misspeled_boolean_literals.baml:1:1]
//  1 │ class TestLiterals {
//    · ▲
//    ╰────
