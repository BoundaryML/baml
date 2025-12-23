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
// Error: Expected RParen, found Dot
//    ╭─[ 0:4:65 ]
//    │
//  4 │   b (int, map<bool, string?>, (char | float)[][] | long_word_123.foobar[])
//    │                                                                 ┬  
//    │                                                                 ╰── Expected RParen, found Dot
//    │ 
//    │ Note: Error code: E0010
// ───╯
// Error: Expected Unexpected token in class body, found Dot
//    ╭─[ 0:4:65 ]
//    │
//  4 │   b (int, map<bool, string?>, (char | float)[][] | long_word_123.foobar[])
//    │                                                                 ┬  
//    │                                                                 ╰── Expected Unexpected token in class body, found Dot
//    │ 
//    │ Note: Error code: E0010
// ───╯
// Error: Expected type, found LBracket
//    ╭─[ 0:4:72 ]
//    │
//  4 │   b (int, map<bool, string?>, (char | float)[][] | long_word_123.foobar[])
//    │                                                                        ┬  
//    │                                                                        ╰── Expected type, found LBracket
//    │ 
//    │ Note: Error code: E0010
// ───╯
// Error: Expected Unexpected token in class body, found RParen
//    ╭─[ 0:4:74 ]
//    │
//  4 │   b (int, map<bool, string?>, (char | float)[][] | long_word_123.foobar[])
//    │                                                                          ┬  
//    │                                                                          ╰── Expected Unexpected token in class body, found RParen
//    │ 
//    │ Note: Error code: E0010
// ───╯
// Error: Expected Greater, found GreaterGreater
//     ╭─[ 0:16:62 ]
//     │
//  16 │   n map<complex_key_type[], map<another_key, (int | string[])>>
//     │                                                              ─┬  
//     │                                                               ╰── Expected Greater, found GreaterGreater
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected Greater, found GreaterGreater
//     ╭─[ 0:16:62 ]
//     │
//  16 │   n map<complex_key_type[], map<another_key, (int | string[])>>
//     │                                                              ─┬  
//     │                                                               ╰── Expected Greater, found GreaterGreater
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected Unexpected token in class body, found GreaterGreater
//     ╭─[ 0:16:62 ]
//     │
//  16 │   n map<complex_key_type[], map<another_key, (int | string[])>>
//     │                                                              ─┬  
//     │                                                               ╰── Expected Unexpected token in class body, found GreaterGreater
//     │ 
//     │ Note: Error code: E0010
// ────╯
