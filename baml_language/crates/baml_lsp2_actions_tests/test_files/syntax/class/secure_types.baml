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
// error: unresolved type: apple_pie
//    ╭─[ class_secure_types.baml:3:4 ]
//    │
//  3 │   a map<string[], (int | bool[]) | apple_pie[][]>
//    │    ───────────────────────┬──────────────────────  
//    │                           ╰──────────────────────── unresolved type: apple_pie
//    │ 
//    │ Note: Error code: E0002
// ───╯
// error: unresolved type: char
//    ╭─[ class_secure_types.baml:4:4 ]
//    │
//  4 │   b (int, map<bool, string?>, (char | float)[][] | long_word_123.foobar[])
//    │    ───────────────────────────────────┬───────────────────────────────────  
//    │                                       ╰───────────────────────────────────── unresolved type: char
//    │ 
//    │ Note: Error code: E0002
// ───╯
// error: unresolved type: long_word_123.foobar
//    ╭─[ class_secure_types.baml:4:4 ]
//    │
//  4 │   b (int, map<bool, string?>, (char | float)[][] | long_word_123.foobar[])
//    │    ───────────────────────────────────┬───────────────────────────────────  
//    │                                       ╰───────────────────────────────────── unresolved type: long_word_123.foobar
//    │ 
//    │ Note: Error code: E0002
// ───╯
// error: unresolved type: apple123_456_pie
//    ╭─[ class_secure_types.baml:5:4 ]
//    │
//  5 │   c apple123_456_pie | (stringer, bool[], (int | char))[]
//    │    ───────────────────────────┬──────────────────────────  
//    │                               ╰──────────────────────────── unresolved type: apple123_456_pie
//    │ 
//    │ Note: Error code: E0002
// ───╯
// error: unresolved type: stringer
//    ╭─[ class_secure_types.baml:5:4 ]
//    │
//  5 │   c apple123_456_pie | (stringer, bool[], (int | char))[]
//    │    ───────────────────────────┬──────────────────────────  
//    │                               ╰──────────────────────────── unresolved type: stringer
//    │ 
//    │ Note: Error code: E0002
// ───╯
// error: unresolved type: char
//    ╭─[ class_secure_types.baml:6:4 ]
//    │
//  6 │   d map<int[][], ((int | float) | char[])>
//    │    ───────────────────┬───────────────────  
//    │                       ╰───────────────────── unresolved type: char
//    │ 
//    │ Note: Error code: E0002
// ───╯
// error: unresolved type: long
//    ╭─[ class_secure_types.baml:7:4 ]
//    │
//  7 │   e ((int, string | char) | ((float, double) | long[], bool)[][][])
//    │    ────────────────────────────────┬───────────────────────────────  
//    │                                    ╰───────────────────────────────── unresolved type: long
//    │ 
//    │ Note: Error code: E0002
// ───╯
// error: unresolved type: VeryLongWord_With_123_Numbers
//    ╭─[ class_secure_types.baml:8:4 ]
//    │
//  8 │   f VeryLongWord_With_123_Numbers[][][][]
//    │    ───────────────────┬──────────────────  
//    │                       ╰──────────────────── unresolved type: VeryLongWord_With_123_Numbers
//    │ 
//    │ Note: Error code: E0002
// ───╯
// error: unresolved type: tuple_inside_tuple
//    ╭─[ class_secure_types.baml:9:4 ]
//    │
//  9 │   g (int, (float, char, bool), string[]) | tuple_inside_tuple[]
//    │    ──────────────────────────────┬─────────────────────────────  
//    │                                  ╰─────────────────────────────── unresolved type: tuple_inside_tuple
//    │ 
//    │ Note: Error code: E0002
// ───╯
// error: unresolved type: char
//     ╭─[ class_secure_types.baml:10:4 ]
//     │
//  10 │   h (((int | string)[]) | map<bool[][], char[]>)
//     │    ──────────────────────┬──────────────────────  
//     │                          ╰──────────────────────── unresolved type: char
//     │ 
//     │ Note: Error code: E0002
// ────╯
// error: unresolved type: apple
//     ╭─[ class_secure_types.baml:11:4 ]
//     │
//  11 │   i (apple, banana | cherry | date_fruit | eggplant_vegetable)[]
//     │    ──────────────────────────────┬──────────────────────────────  
//     │                                  ╰──────────────────────────────── unresolved type: apple
//     │ 
//     │ Note: Error code: E0002
// ────╯
// error: unresolved type: banana
//     ╭─[ class_secure_types.baml:11:4 ]
//     │
//  11 │   i (apple, banana | cherry | date_fruit | eggplant_vegetable)[]
//     │    ──────────────────────────────┬──────────────────────────────  
//     │                                  ╰──────────────────────────────── unresolved type: banana
//     │ 
//     │ Note: Error code: E0002
// ────╯
// error: unresolved type: cherry
//     ╭─[ class_secure_types.baml:11:4 ]
//     │
//  11 │   i (apple, banana | cherry | date_fruit | eggplant_vegetable)[]
//     │    ──────────────────────────────┬──────────────────────────────  
//     │                                  ╰──────────────────────────────── unresolved type: cherry
//     │ 
//     │ Note: Error code: E0002
// ────╯
// error: unresolved type: date_fruit
//     ╭─[ class_secure_types.baml:11:4 ]
//     │
//  11 │   i (apple, banana | cherry | date_fruit | eggplant_vegetable)[]
//     │    ──────────────────────────────┬──────────────────────────────  
//     │                                  ╰──────────────────────────────── unresolved type: date_fruit
//     │ 
//     │ Note: Error code: E0002
// ────╯
// error: unresolved type: eggplant_vegetable
//     ╭─[ class_secure_types.baml:11:4 ]
//     │
//  11 │   i (apple, banana | cherry | date_fruit | eggplant_vegetable)[]
//     │    ──────────────────────────────┬──────────────────────────────  
//     │                                  ╰──────────────────────────────── unresolved type: eggplant_vegetable
//     │ 
//     │ Note: Error code: E0002
// ────╯
// error: unresolved type: char
//     ╭─[ class_secure_types.baml:12:4 ]
//     │
//  12 │   j ((char, int[][], (bool | string[][])) | double[][][][], (float, int)[])
//     │    ────────────────────────────────────┬───────────────────────────────────  
//     │                                        ╰───────────────────────────────────── unresolved type: char
//     │ 
//     │ Note: Error code: E0002
// ────╯
// error: unresolved type: double
//     ╭─[ class_secure_types.baml:12:4 ]
//     │
//  12 │   j ((char, int[][], (bool | string[][])) | double[][][][], (float, int)[])
//     │    ────────────────────────────────────┬───────────────────────────────────  
//     │                                        ╰───────────────────────────────────── unresolved type: double
//     │ 
//     │ Note: Error code: E0002
// ────╯
// error: unresolved type: long
//     ╭─[ class_secure_types.baml:13:4 ]
//     │
//  13 │   k map<string[], (int | long[])> | map<float[][], double[][]>
//     │    ─────────────────────────────┬─────────────────────────────  
//     │                                 ╰─────────────────────────────── unresolved type: long
//     │ 
//     │ Note: Error code: E0002
// ────╯
// error: unresolved type: double
//     ╭─[ class_secure_types.baml:13:4 ]
//     │
//  13 │   k map<string[], (int | long[])> | map<float[][], double[][]>
//     │    ─────────────────────────────┬─────────────────────────────  
//     │                                 ╰─────────────────────────────── unresolved type: double
//     │ 
//     │ Note: Error code: E0002
// ────╯
// error: unresolved type: AlphaNumeric_123_456_789
//     ╭─[ class_secure_types.baml:14:4 ]
//     │
//  14 │   l AlphaNumeric_123_456_789 | (int, bool?) | char[]
//     │    ────────────────────────┬────────────────────────  
//     │                            ╰────────────────────────── unresolved type: AlphaNumeric_123_456_789
//     │ 
//     │ Note: Error code: E0002
// ────╯
// error: unresolved type: char
//     ╭─[ class_secure_types.baml:14:4 ]
//     │
//  14 │   l AlphaNumeric_123_456_789 | (int, bool?) | char[]
//     │    ────────────────────────┬────────────────────────  
//     │                            ╰────────────────────────── unresolved type: char
//     │ 
//     │ Note: Error code: E0002
// ────╯
// error: unresolved type: tuple_1
//     ╭─[ class_secure_types.baml:15:4 ]
//     │
//  15 │   m (tuple_1, tuple_2 | tuple_3, (tuple_4, tuple_5))[]
//     │    ─────────────────────────┬─────────────────────────  
//     │                             ╰─────────────────────────── unresolved type: tuple_1
//     │ 
//     │ Note: Error code: E0002
// ────╯
// error: unresolved type: tuple_2
//     ╭─[ class_secure_types.baml:15:4 ]
//     │
//  15 │   m (tuple_1, tuple_2 | tuple_3, (tuple_4, tuple_5))[]
//     │    ─────────────────────────┬─────────────────────────  
//     │                             ╰─────────────────────────── unresolved type: tuple_2
//     │ 
//     │ Note: Error code: E0002
// ────╯
// error: unresolved type: tuple_3
//     ╭─[ class_secure_types.baml:15:4 ]
//     │
//  15 │   m (tuple_1, tuple_2 | tuple_3, (tuple_4, tuple_5))[]
//     │    ─────────────────────────┬─────────────────────────  
//     │                             ╰─────────────────────────── unresolved type: tuple_3
//     │ 
//     │ Note: Error code: E0002
// ────╯
// error: unresolved type: tuple_4
//     ╭─[ class_secure_types.baml:15:4 ]
//     │
//  15 │   m (tuple_1, tuple_2 | tuple_3, (tuple_4, tuple_5))[]
//     │    ─────────────────────────┬─────────────────────────  
//     │                             ╰─────────────────────────── unresolved type: tuple_4
//     │ 
//     │ Note: Error code: E0002
// ────╯
// error: unresolved type: tuple_5
//     ╭─[ class_secure_types.baml:15:4 ]
//     │
//  15 │   m (tuple_1, tuple_2 | tuple_3, (tuple_4, tuple_5))[]
//     │    ─────────────────────────┬─────────────────────────  
//     │                             ╰─────────────────────────── unresolved type: tuple_5
//     │ 
//     │ Note: Error code: E0002
// ────╯
// error: unresolved type: complex_key_type
//     ╭─[ class_secure_types.baml:16:4 ]
//     │
//  16 │   n map<complex_key_type[], map<another_key, (int | string[])>>
//     │    ──────────────────────────────┬─────────────────────────────  
//     │                                  ╰─────────────────────────────── unresolved type: complex_key_type
//     │ 
//     │ Note: Error code: E0002
// ────╯
// error: unresolved type: another_key
//     ╭─[ class_secure_types.baml:16:4 ]
//     │
//  16 │   n map<complex_key_type[], map<another_key, (int | string[])>>
//     │    ──────────────────────────────┬─────────────────────────────  
//     │                                  ╰─────────────────────────────── unresolved type: another_key
//     │ 
//     │ Note: Error code: E0002
// ────╯
// error: unresolved type: long_long_identifier_123
//     ╭─[ class_secure_types.baml:17:4 ]
//     │
//  17 │   o (((int | string) | bool[]), (float, double) | long_long_identifier_123)
//     │    ────────────────────────────────────┬───────────────────────────────────  
//     │                                        ╰───────────────────────────────────── unresolved type: long_long_identifier_123
//     │ 
//     │ Note: Error code: E0002
// ────╯
