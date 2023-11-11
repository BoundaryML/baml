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







// error: Type `apple_pie` does not exist.
//   -->  class/secure_types.baml:3
//    | 
//  2 | class ComplexTypes {
//  3 |   a {string[]: (int | bool[]) | apple_pie[][]}
//    | 
// error: Type `long_word_123.foobar` does not exist.
//   -->  class/secure_types.baml:4
//    | 
//  3 |   a {string[]: (int | bool[]) | apple_pie[][]}
//  4 |   b (int, {bool: string?}, (char | float)[][] | long_word_123.foobar[])
//    | 
// error: Type `apple123_456_pie` does not exist.
//   -->  class/secure_types.baml:5
//    | 
//  4 |   b (int, {bool: string?}, (char | float)[][] | long_word_123.foobar[])
//  5 |   c apple123_456_pie | (stringer, bool[], (int | char))[]
//    | 
// error: Type `stringer` does not exist. Did you mean `string`?
//   -->  class/secure_types.baml:5
//    | 
//  4 |   b (int, {bool: string?}, (char | float)[][] | long_word_123.foobar[])
//  5 |   c apple123_456_pie | (stringer, bool[], (int | char))[]
//    | 
// error: Type `double` does not exist.
//   -->  class/secure_types.baml:7
//    | 
//  6 |   d {int[][]: ((int | float) | char[])}
//  7 |   e ((int, string | char) | ((float, double) | long[], bool)[][][])
//    | 
// error: Type `long` does not exist.
//   -->  class/secure_types.baml:7
//    | 
//  6 |   d {int[][]: ((int | float) | char[])}
//  7 |   e ((int, string | char) | ((float, double) | long[], bool)[][][])
//    | 
// error: Type `VeryLongWord_With_123_Numbers` does not exist.
//   -->  class/secure_types.baml:8
//    | 
//  7 |   e ((int, string | char) | ((float, double) | long[], bool)[][][])
//  8 |   f VeryLongWord_With_123_Numbers[][][][]
//    | 
// error: Type `tuple_inside_tuple` does not exist.
//   -->  class/secure_types.baml:9
//    | 
//  8 |   f VeryLongWord_With_123_Numbers[][][][]
//  9 |   g (int, (float, char, bool), string[]) | tuple_inside_tuple[]
//    | 
// error: Type `apple` does not exist.
//   -->  class/secure_types.baml:11
//    | 
// 10 |   h (((int | string)[]) | {bool[][]: char[]})
// 11 |   i (apple, banana | cherry | date_fruit | eggplant_vegetable)[]
//    | 
// error: Type `banana` does not exist.
//   -->  class/secure_types.baml:11
//    | 
// 10 |   h (((int | string)[]) | {bool[][]: char[]})
// 11 |   i (apple, banana | cherry | date_fruit | eggplant_vegetable)[]
//    | 
// error: Type `cherry` does not exist.
//   -->  class/secure_types.baml:11
//    | 
// 10 |   h (((int | string)[]) | {bool[][]: char[]})
// 11 |   i (apple, banana | cherry | date_fruit | eggplant_vegetable)[]
//    | 
// error: Type `date_fruit` does not exist.
//   -->  class/secure_types.baml:11
//    | 
// 10 |   h (((int | string)[]) | {bool[][]: char[]})
// 11 |   i (apple, banana | cherry | date_fruit | eggplant_vegetable)[]
//    | 
// error: Type `eggplant_vegetable` does not exist.
//   -->  class/secure_types.baml:11
//    | 
// 10 |   h (((int | string)[]) | {bool[][]: char[]})
// 11 |   i (apple, banana | cherry | date_fruit | eggplant_vegetable)[]
//    | 
// error: Type `double` does not exist.
//   -->  class/secure_types.baml:12
//    | 
// 11 |   i (apple, banana | cherry | date_fruit | eggplant_vegetable)[]
// 12 |   j ((char, int[][], (bool | string[][])) | double[][][][], (float, int)[])
//    | 
// error: Type `long` does not exist.
//   -->  class/secure_types.baml:13
//    | 
// 12 |   j ((char, int[][], (bool | string[][])) | double[][][][], (float, int)[])
// 13 |   k {string[]: (int | long[])} | {float[][]: double[][]}
//    | 
// error: Type `double` does not exist.
//   -->  class/secure_types.baml:13
//    | 
// 12 |   j ((char, int[][], (bool | string[][])) | double[][][][], (float, int)[])
// 13 |   k {string[]: (int | long[])} | {float[][]: double[][]}
//    | 
// error: Type `AlphaNumeric_123_456_789` does not exist.
//   -->  class/secure_types.baml:14
//    | 
// 13 |   k {string[]: (int | long[])} | {float[][]: double[][]}
// 14 |   l AlphaNumeric_123_456_789 | (int, bool?) | char[]
//    | 
// error: Type `tuple_1` does not exist.
//   -->  class/secure_types.baml:15
//    | 
// 14 |   l AlphaNumeric_123_456_789 | (int, bool?) | char[]
// 15 |   m (tuple_1, tuple_2 | tuple_3, (tuple_4, tuple_5))[]
//    | 
// error: Type `tuple_2` does not exist.
//   -->  class/secure_types.baml:15
//    | 
// 14 |   l AlphaNumeric_123_456_789 | (int, bool?) | char[]
// 15 |   m (tuple_1, tuple_2 | tuple_3, (tuple_4, tuple_5))[]
//    | 
// error: Type `tuple_3` does not exist.
//   -->  class/secure_types.baml:15
//    | 
// 14 |   l AlphaNumeric_123_456_789 | (int, bool?) | char[]
// 15 |   m (tuple_1, tuple_2 | tuple_3, (tuple_4, tuple_5))[]
//    | 
// error: Type `tuple_4` does not exist.
//   -->  class/secure_types.baml:15
//    | 
// 14 |   l AlphaNumeric_123_456_789 | (int, bool?) | char[]
// 15 |   m (tuple_1, tuple_2 | tuple_3, (tuple_4, tuple_5))[]
//    | 
// error: Type `tuple_5` does not exist.
//   -->  class/secure_types.baml:15
//    | 
// 14 |   l AlphaNumeric_123_456_789 | (int, bool?) | char[]
// 15 |   m (tuple_1, tuple_2 | tuple_3, (tuple_4, tuple_5))[]
//    | 
// error: Type `another_key` does not exist.
//   -->  class/secure_types.baml:16
//    | 
// 15 |   m (tuple_1, tuple_2 | tuple_3, (tuple_4, tuple_5))[]
// 16 |   n {complex_key_type[]: {another_key: (int | string[])}}
//    | 
// error: Type `complex_key_type` does not exist.
//   -->  class/secure_types.baml:16
//    | 
// 15 |   m (tuple_1, tuple_2 | tuple_3, (tuple_4, tuple_5))[]
// 16 |   n {complex_key_type[]: {another_key: (int | string[])}}
//    | 
// error: Type `double` does not exist.
//   -->  class/secure_types.baml:17
//    | 
// 16 |   n {complex_key_type[]: {another_key: (int | string[])}}
// 17 |   o (((int | string) | bool[]), (float, double) | long_long_identifier_123)
//    | 
// error: Type `long_long_identifier_123` does not exist.
//   -->  class/secure_types.baml:17
//    | 
// 16 |   n {complex_key_type[]: {another_key: (int | string[])}}
// 17 |   o (((int | string) | bool[]), (float, double) | long_long_identifier_123)
//    | 
