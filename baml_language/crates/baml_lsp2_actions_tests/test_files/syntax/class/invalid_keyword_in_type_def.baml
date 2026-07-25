class OkClass {
  field string
}

enum OkEnum {
  A
  B
}

classs WrongClass {
  field string
}

random_keyword WrongEnum {
  A
  B
}

// error: Error validating: Unexpected keyword 'classs' in type definition. Use 'class' or 'enum'.
//   -->  class/invalid_keyword_in_type_def.baml:10
//    |
//  9 |
// 10 | classs WrongClass {
//    |
// error: Error validating: Unexpected keyword 'random_keyword' in type definition. Use 'class' or 'enum'.
//   -->  class/invalid_keyword_in_type_def.baml:14
//    |
// 13 |
// 14 | random_keyword WrongEnum {
//    |

//----
//- diagnostics
// E0010
//
//   × invalid syntax
//     ╭─[class_invalid_keyword_in_type_def.baml:10:1]
//  10 │ classs WrongClass {
//     · ───┬──
//     ·    ╰── unknown keyword `classs`; expected `class`, `enum`, `function`, `client`, `generator`, `test`, or `type`
//     ╰────
// E0010
//
//   × invalid syntax
//     ╭─[class_invalid_keyword_in_type_def.baml:14:1]
//  14 │ random_keyword WrongEnum {
//     · ───────┬──────
//     ·        ╰── unknown keyword `random_keyword`; expected `class`, `enum`, `function`, `client`, `generator`, `test`, or `type`
//     ╰────
