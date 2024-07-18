// Tests for validating complex types
class ComplexTypes {
  a {string[]: (int | bool[]) | apple_pie[][]}
  b (int, {bool: string?}, (char | float)[][] | long_word_123.foobar[])
  c apple123_456_pie | (stringer, bool[], (int | char))[]
  d {int[][]: ((int | float) | char[])}
  e ((int, string | char) | ((float, double) | long[], bool)[][][])
  f VeryLongWord_With_123_Numbers[][][][]
  g (int, (float, char, bool), string[]) | tuple_inside_tuple[]
  h (((int | string)[]) | {bool[][]: char[]})
  i (apple, banana | cherry | date_fruit | eggplant_vegetable)[]
  j ((char, int[][], (bool | string[][])) | double[][][][], (float, int)[])
  k {string[]: (int | long[])} | {float[][]: double[][]}
  l AlphaNumeric_123_456_789 | (int, bool?) | char[]
  m (tuple_1, tuple_2 | tuple_3, (tuple_4, tuple_5))[]
  n {complex_key_type[]: {another_key: (int | string[])}}
  o (((int | string) | bool[]), (float, double) | long_long_identifier_123)
}

// error: Error validating: This line is not a valid field or attribute definition. A valid class property looks like: 'myProperty string[] @description("This is a description")'
//   -->  class/secure_types.baml:3
//    | 
//  2 | class ComplexTypes {
//  3 |   a {string[]: (int | bool[]) | apple_pie[][]}
//  4 |   b (int, {bool: string?}, (char | float)[][] | long_word_123.foobar[])
//    | 
// error: Error validating: This line is not a valid field or attribute definition. A valid class property looks like: 'myProperty string[] @description("This is a description")'
//   -->  class/secure_types.baml:4
//    | 
//  3 |   a {string[]: (int | bool[]) | apple_pie[][]}
//  4 |   b (int, {bool: string?}, (char | float)[][] | long_word_123.foobar[])
//  5 |   c apple123_456_pie | (stringer, bool[], (int | char))[]
//    | 
// error: Error validating: This line is not a valid field or attribute definition. A valid class property looks like: 'myProperty string[] @description("This is a description")'
//   -->  class/secure_types.baml:6
//    | 
//  5 |   c apple123_456_pie | (stringer, bool[], (int | char))[]
//  6 |   d {int[][]: ((int | float) | char[])}
//  7 |   e ((int, string | char) | ((float, double) | long[], bool)[][][])
//    | 
// error: Error validating: This line is not a valid field or attribute definition. A valid class property looks like: 'myProperty string[] @description("This is a description")'
//   -->  class/secure_types.baml:10
//    | 
//  9 |   g (int, (float, char, bool), string[]) | tuple_inside_tuple[]
// 10 |   h (((int | string)[]) | {bool[][]: char[]})
// 11 |   i (apple, banana | cherry | date_fruit | eggplant_vegetable)[]
//    | 
// error: Error validating: This line is not a valid field or attribute definition. A valid class property looks like: 'myProperty string[] @description("This is a description")'
//   -->  class/secure_types.baml:13
//    | 
// 12 |   j ((char, int[][], (bool | string[][])) | double[][][][], (float, int)[])
// 13 |   k {string[]: (int | long[])} | {float[][]: double[][]}
// 14 |   l AlphaNumeric_123_456_789 | (int, bool?) | char[]
//    | 
// error: Error validating: This line is not a valid field or attribute definition. A valid class property looks like: 'myProperty string[] @description("This is a description")'
//   -->  class/secure_types.baml:16
//    | 
// 15 |   m (tuple_1, tuple_2 | tuple_3, (tuple_4, tuple_5))[]
// 16 |   n {complex_key_type[]: {another_key: (int | string[])}}
// 17 |   o (((int | string) | bool[]), (float, double) | long_long_identifier_123)
//    | 
