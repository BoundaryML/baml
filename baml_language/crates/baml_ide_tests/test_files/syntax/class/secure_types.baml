// Tests for validating complex types
class ComplexTypes {
  a map<string[], (int | bool[]) | apple_pie[][]>
  b (int, map<bool, string?>, (char | float)[][] | long_word_123.foobar[])
  c apple123_456_pie | (stringer, bool[], (int | char))[]
  d map<int[][], ((int | float) | char[])>
  e ((int, string | char) | ((float, double) | long[], bool)[][][])
  f VeryLongWord_With_123_Numbers[][][][]
  g (int, (float, char, bool), string[]) | tuple_inside_tuple[]
  h (((int | string)[]) | map<bool[][], char[]>)
  i (apple, banana | cherry | date_fruit | eggplant_vegetable)[]
  j ((char, int[][], (bool | string[][])) | double[][][][], (float, int)[])
  k map<string[], (int | long[])> | map<float[][], double[][]>
  l AlphaNumeric_123_456_789 | (int, bool?) | char[]
  m (tuple_1, tuple_2 | tuple_3, (tuple_4, tuple_5))[]
  n map<complex_key_type[], map<another_key, (int | string[])>>
  o (((int | string) | bool[]), (float, double) | long_long_identifier_123)
  p string[]?
  q map<string, string>?
}

// error: Error validating: Path identifiers are not supported in base types
//   -->  class/secure_types.baml:4
//    | 
//  3 |   a map<string[], (int | bool[]) | apple_pie[][]>
//  4 |   b (int, map<bool, string?>, (char | float)[][] | long_word_123.foobar[])
//    |

//----
//- diagnostics
// Error: Expected ')', found '.'
//    ╭─[ class_secure_types.baml:4:65 ]
//    │
//  4 │   b (int, map<bool, string?>, (char | float)[][] | long_word_123.foobar[])
//    │                                                                 ┬  
//    │                                                                 ╰── Expected ')', found '.'
//    │ 
//    │ Note: Error code: E0010
// ───╯
// Error: Expected Unexpected token in class body, found '.'
//    ╭─[ class_secure_types.baml:4:65 ]
//    │
//  4 │   b (int, map<bool, string?>, (char | float)[][] | long_word_123.foobar[])
//    │                                                                 ┬  
//    │                                                                 ╰── Expected Unexpected token in class body, found '.'
//    │ 
//    │ Note: Error code: E0010
// ───╯
// Error: field 'foobar' is missing a type annotation
//    ╭─[ class_secure_types.baml:4:66 ]
//    │
//  4 │   b (int, map<bool, string?>, (char | float)[][] | long_word_123.foobar[])
//    │                                                                  ───┬──  
//    │                                                                     ╰──── field 'foobar' is missing a type annotation
//    │ 
//    │ Note: Error code: E0010
// ───╯
// Error: Expected Unexpected token in class body, found '['
//    ╭─[ class_secure_types.baml:4:72 ]
//    │
//  4 │   b (int, map<bool, string?>, (char | float)[][] | long_word_123.foobar[])
//    │                                                                        ┬  
//    │                                                                        ╰── Expected Unexpected token in class body, found '['
//    │ 
//    │ Note: Error code: E0010
// ───╯
// Error: Expected Unexpected token in class body, found ']'
//    ╭─[ class_secure_types.baml:4:73 ]
//    │
//  4 │   b (int, map<bool, string?>, (char | float)[][] | long_word_123.foobar[])
//    │                                                                         ┬  
//    │                                                                         ╰── Expected Unexpected token in class body, found ']'
//    │ 
//    │ Note: Error code: E0010
// ───╯
// Error: Expected Unexpected token in class body, found ')'
//    ╭─[ class_secure_types.baml:4:74 ]
//    │
//  4 │   b (int, map<bool, string?>, (char | float)[][] | long_word_123.foobar[])
//    │                                                                          ┬  
//    │                                                                          ╰── Expected Unexpected token in class body, found ')'
//    │ 
//    │ Note: Error code: E0010
// ───╯
// Error: Expected '>', found '>>'
//     ╭─[ class_secure_types.baml:16:62 ]
//     │
//  16 │   n map<complex_key_type[], map<another_key, (int | string[])>>
//     │                                                              ─┬  
//     │                                                               ╰── Expected '>', found '>>'
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected '>', found '>>'
//     ╭─[ class_secure_types.baml:16:62 ]
//     │
//  16 │   n map<complex_key_type[], map<another_key, (int | string[])>>
//     │                                                              ─┬  
//     │                                                               ╰── Expected '>', found '>>'
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected Unexpected token in class body, found '>>'
//     ╭─[ class_secure_types.baml:16:62 ]
//     │
//  16 │   n map<complex_key_type[], map<another_key, (int | string[])>>
//     │                                                              ─┬  
//     │                                                               ╰── Expected Unexpected token in class body, found '>>'
//     │ 
//     │ Note: Error code: E0010
// ────╯
