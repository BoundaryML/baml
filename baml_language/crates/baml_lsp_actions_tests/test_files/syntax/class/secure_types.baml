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
// Error: Path identifiers (e.g., 'a.b') are not supported in type expressions
//    ╭─[ class_secure_types.baml:4:65 ]
//    │
//  4 │   b (int, map<bool, string?>, (char | float)[][] | long_word_123.foobar[])
//    │                                                                 ┬  
//    │                                                                 ╰── Path identifiers (e.g., 'a.b') are not supported in type expressions
//    │ 
//    │ Note: Error code: E0010
// ───╯
// Error: Unknown type `apple_pie`
//    ╭─[ class_secure_types.baml:3:4 ]
//    │
//  3 │   a map<string[], (int | bool[]) | apple_pie[][]>
//    │    ───────────────────────┬──────────────────────  
//    │                           ╰──────────────────────── Unknown type `apple_pie`
//    │ 
//    │ Note: Error code: E0002
// ───╯
// Error: Unknown type `char`
//    ╭─[ class_secure_types.baml:4:4 ]
//    │
//  4 │   b (int, map<bool, string?>, (char | float)[][] | long_word_123.foobar[])
//    │    ───────────────────────────────────┬───────────────────────────────────  
//    │                                       ╰───────────────────────────────────── Unknown type `char`
//    │ 
//    │ Note: Error code: E0002
// ───╯
// Error: Unknown type `long_word_123`
//    ╭─[ class_secure_types.baml:4:4 ]
//    │
//  4 │   b (int, map<bool, string?>, (char | float)[][] | long_word_123.foobar[])
//    │    ───────────────────────────────────┬───────────────────────────────────  
//    │                                       ╰───────────────────────────────────── Unknown type `long_word_123`
//    │ 
//    │ Note: Error code: E0002
// ───╯
// Error: Unknown type `apple123_456_pie`
//    ╭─[ class_secure_types.baml:5:4 ]
//    │
//  5 │   c apple123_456_pie | (stringer, bool[], (int | char))[]
//    │    ───────────────────────────┬──────────────────────────  
//    │                               ╰──────────────────────────── Unknown type `apple123_456_pie`
//    │ 
//    │ Note: Error code: E0002
// ───╯
// Error: Unknown type `stringer`
//    ╭─[ class_secure_types.baml:5:4 ]
//    │
//  5 │   c apple123_456_pie | (stringer, bool[], (int | char))[]
//    │    ───────────────────────────┬──────────────────────────  
//    │                               ╰──────────────────────────── Unknown type `stringer`
//    │ 
//    │ Note: Error code: E0002
// ───╯
// Error: Unknown type `char`
//    ╭─[ class_secure_types.baml:6:4 ]
//    │
//  6 │   d map<int[][], ((int | float) | char[])>
//    │    ───────────────────┬───────────────────  
//    │                       ╰───────────────────── Unknown type `char`
//    │ 
//    │ Note: Error code: E0002
// ───╯
// Error: Unknown type `long`
//    ╭─[ class_secure_types.baml:7:4 ]
//    │
//  7 │   e ((int, string | char) | ((float, double) | long[], bool)[][][])
//    │    ────────────────────────────────┬───────────────────────────────  
//    │                                    ╰───────────────────────────────── Unknown type `long`
//    │ 
//    │ Note: Error code: E0002
// ───╯
// Error: Unknown type `VeryLongWord_With_123_Numbers`
//    ╭─[ class_secure_types.baml:8:4 ]
//    │
//  8 │   f VeryLongWord_With_123_Numbers[][][][]
//    │    ───────────────────┬──────────────────  
//    │                       ╰──────────────────── Unknown type `VeryLongWord_With_123_Numbers`
//    │ 
//    │ Note: Error code: E0002
// ───╯
// Error: Unknown type `tuple_inside_tuple`
//    ╭─[ class_secure_types.baml:9:4 ]
//    │
//  9 │   g (int, (float, char, bool), string[]) | tuple_inside_tuple[]
//    │    ──────────────────────────────┬─────────────────────────────  
//    │                                  ╰─────────────────────────────── Unknown type `tuple_inside_tuple`
//    │ 
//    │ Note: Error code: E0002
// ───╯
// Error: Unknown type `char`
//     ╭─[ class_secure_types.baml:10:4 ]
//     │
//  10 │   h (((int | string)[]) | map<bool[][], char[]>)
//     │    ──────────────────────┬──────────────────────  
//     │                          ╰──────────────────────── Unknown type `char`
//     │ 
//     │ Note: Error code: E0002
// ────╯
// Error: Unknown type `char`
//     ╭─[ class_secure_types.baml:12:4 ]
//     │
//  12 │   j ((char, int[][], (bool | string[][])) | double[][][][], (float, int)[])
//     │    ────────────────────────────────────┬───────────────────────────────────  
//     │                                        ╰───────────────────────────────────── Unknown type `char`
//     │ 
//     │ Note: Error code: E0002
// ────╯
// Error: Unknown type `double`
//     ╭─[ class_secure_types.baml:12:4 ]
//     │
//  12 │   j ((char, int[][], (bool | string[][])) | double[][][][], (float, int)[])
//     │    ────────────────────────────────────┬───────────────────────────────────  
//     │                                        ╰───────────────────────────────────── Unknown type `double`
//     │ 
//     │ Note: Error code: E0002
// ────╯
// Error: Unknown type `long`
//     ╭─[ class_secure_types.baml:13:4 ]
//     │
//  13 │   k map<string[], (int | long[])> | map<float[][], double[][]>
//     │    ─────────────────────────────┬─────────────────────────────  
//     │                                 ╰─────────────────────────────── Unknown type `long`
//     │ 
//     │ Note: Error code: E0002
// ────╯
// Error: Unknown type `double`
//     ╭─[ class_secure_types.baml:13:4 ]
//     │
//  13 │   k map<string[], (int | long[])> | map<float[][], double[][]>
//     │    ─────────────────────────────┬─────────────────────────────  
//     │                                 ╰─────────────────────────────── Unknown type `double`
//     │ 
//     │ Note: Error code: E0002
// ────╯
// Error: Unknown type `AlphaNumeric_123_456_789`
//     ╭─[ class_secure_types.baml:14:4 ]
//     │
//  14 │   l AlphaNumeric_123_456_789 | (int, bool?) | char[]
//     │    ────────────────────────┬────────────────────────  
//     │                            ╰────────────────────────── Unknown type `AlphaNumeric_123_456_789`
//     │ 
//     │ Note: Error code: E0002
// ────╯
// Error: Unknown type `char`
//     ╭─[ class_secure_types.baml:14:4 ]
//     │
//  14 │   l AlphaNumeric_123_456_789 | (int, bool?) | char[]
//     │    ────────────────────────┬────────────────────────  
//     │                            ╰────────────────────────── Unknown type `char`
//     │ 
//     │ Note: Error code: E0002
// ────╯
// Error: Unknown type `complex_key_type`
//     ╭─[ class_secure_types.baml:16:4 ]
//     │
//  16 │   n map<complex_key_type[], map<another_key, (int | string[])>>
//     │    ──────────────────────────────┬─────────────────────────────  
//     │                                  ╰─────────────────────────────── Unknown type `complex_key_type`
//     │ 
//     │ Note: Error code: E0002
// ────╯
// Error: Unknown type `another_key`
//     ╭─[ class_secure_types.baml:16:4 ]
//     │
//  16 │   n map<complex_key_type[], map<another_key, (int | string[])>>
//     │    ──────────────────────────────┬─────────────────────────────  
//     │                                  ╰─────────────────────────────── Unknown type `another_key`
//     │ 
//     │ Note: Error code: E0002
// ────╯
// Error: Unknown type `long_long_identifier_123`
//     ╭─[ class_secure_types.baml:17:4 ]
//     │
//  17 │   o (((int | string) | bool[]), (float, double) | long_long_identifier_123)
//     │    ────────────────────────────────────┬───────────────────────────────────  
//     │                                        ╰───────────────────────────────────── Unknown type `long_long_identifier_123`
//     │ 
//     │ Note: Error code: E0002
// ────╯
