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
}

// error: Error validating: Maps may only have strings as keys
//   -->  class/secure_types.baml:3
//    | 
//  2 | class ComplexTypes {
//  3 |   a map<string[], (int | bool[]) | apple_pie[][]>
//    | 
// error: Error validating: Maps may only have strings as keys
//   -->  class/secure_types.baml:6
//    | 
//  5 |   c apple123_456_pie | (stringer, bool[], (int | char))[]
//  6 |   d map<int[][], ((int | float) | char[])>
//    | 
// error: Error validating: Maps may only have strings as keys
//   -->  class/secure_types.baml:10
//    | 
//  9 |   g (int, (float, char, bool), string[]) | tuple_inside_tuple[]
// 10 |   h (((int | string)[]) | map<bool[][], char[]>)
//    | 
// error: Error validating: Maps may only have strings as keys
//   -->  class/secure_types.baml:13
//    | 
// 12 |   j ((char, int[][], (bool | string[][])) | double[][][][], (float, int)[])
// 13 |   k map<string[], (int | long[])> | map<float[][], double[][]>
//    | 
// error: Error validating: Maps may only have strings as keys
//   -->  class/secure_types.baml:13
//    | 
// 12 |   j ((char, int[][], (bool | string[][])) | double[][][][], (float, int)[])
// 13 |   k map<string[], (int | long[])> | map<float[][], double[][]>
//    | 
// error: Error validating: Maps may only have strings as keys
//   -->  class/secure_types.baml:16
//    | 
// 15 |   m (tuple_1, tuple_2 | tuple_3, (tuple_4, tuple_5))[]
// 16 |   n map<complex_key_type[], map<another_key, (int | string[])>>
//    | 
// error: Error validating: Maps may only have strings as keys
//   -->  class/secure_types.baml:16
//    | 
// 15 |   m (tuple_1, tuple_2 | tuple_3, (tuple_4, tuple_5))[]
// 16 |   n map<complex_key_type[], map<another_key, (int | string[])>>
//    | 
