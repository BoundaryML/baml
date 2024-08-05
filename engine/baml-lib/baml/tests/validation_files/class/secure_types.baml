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

// error: Type `apple_pie` does not exist. Did you mean one of these: `ComplexTypes`, `float`, `bool`, `string`, `int`? (error.rs l.499)
//   -->  class/secure_types.baml:3
//    | 
//  2 | class ComplexTypes {
//  3 |   a map<string[], (int | bool[]) | apple_pie[][]>
//    | 
// error: Error validating: Maps may only have strings as keys
//   -->  class/secure_types.baml:3
//    | 
//  2 | class ComplexTypes {
//  3 |   a map<string[], (int | bool[]) | apple_pie[][]>
//    | 
// error: Type apple_pie not found. Did you mean one of these: `audio`, `ComplexTypes`, `float`? (from error.rs l.422)
//   -->  class/secure_types.baml:3
//    | 
//  2 | class ComplexTypes {
//  3 |   a map<string[], (int | bool[]) | apple_pie[][]>
//    | 
// error: Type `char` does not exist. Did you mean one of these: `int`, `float`, `bool`, `string`, `ComplexTypes`? (error.rs l.499)
//   -->  class/secure_types.baml:4
//    | 
//  3 |   a map<string[], (int | bool[]) | apple_pie[][]>
//  4 |   b (int, map<bool, string?>, (char | float)[][] | long_word_123.foobar[])
//    | 
// error: Type `long_word_123.foobar` does not exist. Did you mean one of these: `float`, `bool`, `ComplexTypes`, `string`, `int`? (error.rs l.499)
//   -->  class/secure_types.baml:4
//    | 
//  3 |   a map<string[], (int | bool[]) | apple_pie[][]>
//  4 |   b (int, map<bool, string?>, (char | float)[][] | long_word_123.foobar[])
//    | 
// error: Type char not found. Did you mean one of these: `int`, `float`, `bool`? (from error.rs l.422)
//   -->  class/secure_types.baml:4
//    | 
//  3 |   a map<string[], (int | bool[]) | apple_pie[][]>
//  4 |   b (int, map<bool, string?>, (char | float)[][] | long_word_123.foobar[])
//    | 
// error: Type long_word_123.foobar not found. Did you mean one of these: `float`, `bool`, `audio`? (from error.rs l.422)
//   -->  class/secure_types.baml:4
//    | 
//  3 |   a map<string[], (int | bool[]) | apple_pie[][]>
//  4 |   b (int, map<bool, string?>, (char | float)[][] | long_word_123.foobar[])
//    | 
// error: Type `apple123_456_pie` does not exist. Did you mean one of these: `ComplexTypes`, `float`, `bool`, `string`, `int`? (error.rs l.499)
//   -->  class/secure_types.baml:5
//    | 
//  4 |   b (int, map<bool, string?>, (char | float)[][] | long_word_123.foobar[])
//  5 |   c apple123_456_pie | (stringer, bool[], (int | char))[]
//    | 
// error: Type `stringer` does not exist. Did you mean one of these: `string`, `int`, `float`, `bool`, `ComplexTypes`? (error.rs l.499)
//   -->  class/secure_types.baml:5
//    | 
//  4 |   b (int, map<bool, string?>, (char | float)[][] | long_word_123.foobar[])
//  5 |   c apple123_456_pie | (stringer, bool[], (int | char))[]
//    | 
// error: Type `char` does not exist. Did you mean one of these: `int`, `float`, `bool`, `string`, `ComplexTypes`? (error.rs l.499)
//   -->  class/secure_types.baml:5
//    | 
//  4 |   b (int, map<bool, string?>, (char | float)[][] | long_word_123.foobar[])
//  5 |   c apple123_456_pie | (stringer, bool[], (int | char))[]
//    | 
// error: Type apple123_456_pie not found. Did you mean one of these: `ComplexTypes`, `audio`, `float`? (from error.rs l.422)
//   -->  class/secure_types.baml:5
//    | 
//  4 |   b (int, map<bool, string?>, (char | float)[][] | long_word_123.foobar[])
//  5 |   c apple123_456_pie | (stringer, bool[], (int | char))[]
//    | 
// error: Type stringer not found. Did you mean one of these: `string`, `int`, `image`? (from error.rs l.422)
//   -->  class/secure_types.baml:5
//    | 
//  4 |   b (int, map<bool, string?>, (char | float)[][] | long_word_123.foobar[])
//  5 |   c apple123_456_pie | (stringer, bool[], (int | char))[]
//    | 
// error: Type char not found. Did you mean one of these: `int`, `float`, `bool`? (from error.rs l.422)
//   -->  class/secure_types.baml:5
//    | 
//  4 |   b (int, map<bool, string?>, (char | float)[][] | long_word_123.foobar[])
//  5 |   c apple123_456_pie | (stringer, bool[], (int | char))[]
//    | 
// error: Type `char` does not exist. Did you mean one of these: `int`, `float`, `bool`, `string`, `ComplexTypes`? (error.rs l.499)
//   -->  class/secure_types.baml:6
//    | 
//  5 |   c apple123_456_pie | (stringer, bool[], (int | char))[]
//  6 |   d map<int[][], ((int | float) | char[])>
//    | 
// error: Error validating: Maps may only have strings as keys
//   -->  class/secure_types.baml:6
//    | 
//  5 |   c apple123_456_pie | (stringer, bool[], (int | char))[]
//  6 |   d map<int[][], ((int | float) | char[])>
//    | 
// error: Type char not found. Did you mean one of these: `int`, `float`, `bool`? (from error.rs l.422)
//   -->  class/secure_types.baml:6
//    | 
//  5 |   c apple123_456_pie | (stringer, bool[], (int | char))[]
//  6 |   d map<int[][], ((int | float) | char[])>
//    | 
// error: Type `char` does not exist. Did you mean one of these: `int`, `float`, `bool`, `string`, `ComplexTypes`? (error.rs l.499)
//   -->  class/secure_types.baml:7
//    | 
//  6 |   d map<int[][], ((int | float) | char[])>
//  7 |   e ((int, string | char) | ((float, double) | long[], bool)[][][])
//    | 
// error: Type `double` does not exist. Did you mean one of these: `bool`, `string`, `int`, `float`, `ComplexTypes`? (error.rs l.499)
//   -->  class/secure_types.baml:7
//    | 
//  6 |   d map<int[][], ((int | float) | char[])>
//  7 |   e ((int, string | char) | ((float, double) | long[], bool)[][][])
//    | 
// error: Type `long` does not exist. Did you mean one of these: `int`, `float`, `bool`, `string`, `ComplexTypes`? (error.rs l.499)
//   -->  class/secure_types.baml:7
//    | 
//  6 |   d map<int[][], ((int | float) | char[])>
//  7 |   e ((int, string | char) | ((float, double) | long[], bool)[][][])
//    | 
// error: Type char not found. Did you mean one of these: `int`, `float`, `bool`? (from error.rs l.422)
//   -->  class/secure_types.baml:7
//    | 
//  6 |   d map<int[][], ((int | float) | char[])>
//  7 |   e ((int, string | char) | ((float, double) | long[], bool)[][][])
//    | 
// error: Type double not found. Did you mean one of these: `bool`, `null`, `image`? (from error.rs l.422)
//   -->  class/secure_types.baml:7
//    | 
//  6 |   d map<int[][], ((int | float) | char[])>
//  7 |   e ((int, string | char) | ((float, double) | long[], bool)[][][])
//    | 
// error: Type long not found. Did you mean one of these: `int`, `float`, `bool`? (from error.rs l.422)
//   -->  class/secure_types.baml:7
//    | 
//  6 |   d map<int[][], ((int | float) | char[])>
//  7 |   e ((int, string | char) | ((float, double) | long[], bool)[][][])
//    | 
// error: Type `VeryLongWord_With_123_Numbers` does not exist.
//   -->  class/secure_types.baml:8
//    | 
//  7 |   e ((int, string | char) | ((float, double) | long[], bool)[][][])
//  8 |   f VeryLongWord_With_123_Numbers[][][][]
//    | 
// error: Type VeryLongWord_With_123_Numbers not found. (from error.rs l.422)
//   -->  class/secure_types.baml:8
//    | 
//  7 |   e ((int, string | char) | ((float, double) | long[], bool)[][][])
//  8 |   f VeryLongWord_With_123_Numbers[][][][]
//    | 
// error: Type `char` does not exist. Did you mean one of these: `int`, `float`, `bool`, `string`, `ComplexTypes`? (error.rs l.499)
//   -->  class/secure_types.baml:9
//    | 
//  8 |   f VeryLongWord_With_123_Numbers[][][][]
//  9 |   g (int, (float, char, bool), string[]) | tuple_inside_tuple[]
//    | 
// error: Type `tuple_inside_tuple` does not exist. Did you mean one of these: `ComplexTypes`, `int`, `string`, `float`, `bool`? (error.rs l.499)
//   -->  class/secure_types.baml:9
//    | 
//  8 |   f VeryLongWord_With_123_Numbers[][][][]
//  9 |   g (int, (float, char, bool), string[]) | tuple_inside_tuple[]
//    | 
// error: Type char not found. Did you mean one of these: `int`, `float`, `bool`? (from error.rs l.422)
//   -->  class/secure_types.baml:9
//    | 
//  8 |   f VeryLongWord_With_123_Numbers[][][][]
//  9 |   g (int, (float, char, bool), string[]) | tuple_inside_tuple[]
//    | 
// error: Type tuple_inside_tuple not found. Did you mean one of these: `ComplexTypes`, `int`, `null`? (from error.rs l.422)
//   -->  class/secure_types.baml:9
//    | 
//  8 |   f VeryLongWord_With_123_Numbers[][][][]
//  9 |   g (int, (float, char, bool), string[]) | tuple_inside_tuple[]
//    | 
// error: Type `char` does not exist. Did you mean one of these: `int`, `float`, `bool`, `string`, `ComplexTypes`? (error.rs l.499)
//   -->  class/secure_types.baml:10
//    | 
//  9 |   g (int, (float, char, bool), string[]) | tuple_inside_tuple[]
// 10 |   h (((int | string)[]) | map<bool[][], char[]>)
//    | 
// error: Error validating: Maps may only have strings as keys
//   -->  class/secure_types.baml:10
//    | 
//  9 |   g (int, (float, char, bool), string[]) | tuple_inside_tuple[]
// 10 |   h (((int | string)[]) | map<bool[][], char[]>)
//    | 
// error: Type char not found. Did you mean one of these: `int`, `float`, `bool`? (from error.rs l.422)
//   -->  class/secure_types.baml:10
//    | 
//  9 |   g (int, (float, char, bool), string[]) | tuple_inside_tuple[]
// 10 |   h (((int | string)[]) | map<bool[][], char[]>)
//    | 
// error: Type `apple` does not exist. Did you mean one of these: `bool`, `int`, `float`, `string`, `ComplexTypes`? (error.rs l.499)
//   -->  class/secure_types.baml:11
//    | 
// 10 |   h (((int | string)[]) | map<bool[][], char[]>)
// 11 |   i (apple, banana | cherry | date_fruit | eggplant_vegetable)[]
//    | 
// error: Type `banana` does not exist. Did you mean one of these: `string`, `int`, `float`, `bool`, `ComplexTypes`? (error.rs l.499)
//   -->  class/secure_types.baml:11
//    | 
// 10 |   h (((int | string)[]) | map<bool[][], char[]>)
// 11 |   i (apple, banana | cherry | date_fruit | eggplant_vegetable)[]
//    | 
// error: Type `cherry` does not exist. Did you mean one of these: `string`, `int`, `float`, `bool`, `ComplexTypes`? (error.rs l.499)
//   -->  class/secure_types.baml:11
//    | 
// 10 |   h (((int | string)[]) | map<bool[][], char[]>)
// 11 |   i (apple, banana | cherry | date_fruit | eggplant_vegetable)[]
//    | 
// error: Type `date_fruit` does not exist. Did you mean one of these: `string`, `float`, `int`, `bool`, `ComplexTypes`? (error.rs l.499)
//   -->  class/secure_types.baml:11
//    | 
// 10 |   h (((int | string)[]) | map<bool[][], char[]>)
// 11 |   i (apple, banana | cherry | date_fruit | eggplant_vegetable)[]
//    | 
// error: Type `eggplant_vegetable` does not exist. Did you mean one of these: `ComplexTypes`, `string`, `int`, `float`, `bool`? (error.rs l.499)
//   -->  class/secure_types.baml:11
//    | 
// 10 |   h (((int | string)[]) | map<bool[][], char[]>)
// 11 |   i (apple, banana | cherry | date_fruit | eggplant_vegetable)[]
//    | 
// error: Type apple not found. Did you mean one of these: `bool`, `image`, `audio`? (from error.rs l.422)
//   -->  class/secure_types.baml:11
//    | 
// 10 |   h (((int | string)[]) | map<bool[][], char[]>)
// 11 |   i (apple, banana | cherry | date_fruit | eggplant_vegetable)[]
//    | 
// error: Type banana not found. Did you mean one of these: `int`, `float`, `bool`? (from error.rs l.422)
//   -->  class/secure_types.baml:11
//    | 
// 10 |   h (((int | string)[]) | map<bool[][], char[]>)
// 11 |   i (apple, banana | cherry | date_fruit | eggplant_vegetable)[]
//    | 
// error: Type cherry not found. Did you mean one of these: `int`, `float`, `bool`? (from error.rs l.422)
//   -->  class/secure_types.baml:11
//    | 
// 10 |   h (((int | string)[]) | map<bool[][], char[]>)
// 11 |   i (apple, banana | cherry | date_fruit | eggplant_vegetable)[]
//    | 
// error: Type date_fruit not found. Did you mean one of these: `float`, `string`, `audio`? (from error.rs l.422)
//   -->  class/secure_types.baml:11
//    | 
// 10 |   h (((int | string)[]) | map<bool[][], char[]>)
// 11 |   i (apple, banana | cherry | date_fruit | eggplant_vegetable)[]
//    | 
// error: Type eggplant_vegetable not found. Did you mean one of these: `ComplexTypes`, `image`, `int`? (from error.rs l.422)
//   -->  class/secure_types.baml:11
//    | 
// 10 |   h (((int | string)[]) | map<bool[][], char[]>)
// 11 |   i (apple, banana | cherry | date_fruit | eggplant_vegetable)[]
//    | 
// error: Type `char` does not exist. Did you mean one of these: `int`, `float`, `bool`, `string`, `ComplexTypes`? (error.rs l.499)
//   -->  class/secure_types.baml:12
//    | 
// 11 |   i (apple, banana | cherry | date_fruit | eggplant_vegetable)[]
// 12 |   j ((char, int[][], (bool | string[][])) | double[][][][], (float, int)[])
//    | 
// error: Type `double` does not exist. Did you mean one of these: `bool`, `string`, `int`, `float`, `ComplexTypes`? (error.rs l.499)
//   -->  class/secure_types.baml:12
//    | 
// 11 |   i (apple, banana | cherry | date_fruit | eggplant_vegetable)[]
// 12 |   j ((char, int[][], (bool | string[][])) | double[][][][], (float, int)[])
//    | 
// error: Type char not found. Did you mean one of these: `int`, `float`, `bool`? (from error.rs l.422)
//   -->  class/secure_types.baml:12
//    | 
// 11 |   i (apple, banana | cherry | date_fruit | eggplant_vegetable)[]
// 12 |   j ((char, int[][], (bool | string[][])) | double[][][][], (float, int)[])
//    | 
// error: Type double not found. Did you mean one of these: `bool`, `null`, `image`? (from error.rs l.422)
//   -->  class/secure_types.baml:12
//    | 
// 11 |   i (apple, banana | cherry | date_fruit | eggplant_vegetable)[]
// 12 |   j ((char, int[][], (bool | string[][])) | double[][][][], (float, int)[])
//    | 
// error: Type `long` does not exist. Did you mean one of these: `int`, `float`, `bool`, `string`, `ComplexTypes`? (error.rs l.499)
//   -->  class/secure_types.baml:13
//    | 
// 12 |   j ((char, int[][], (bool | string[][])) | double[][][][], (float, int)[])
// 13 |   k map<string[], (int | long[])> | map<float[][], double[][]>
//    | 
// error: Type `double` does not exist. Did you mean one of these: `bool`, `string`, `int`, `float`, `ComplexTypes`? (error.rs l.499)
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
// error: Type long not found. Did you mean one of these: `int`, `float`, `bool`? (from error.rs l.422)
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
// error: Type double not found. Did you mean one of these: `bool`, `null`, `image`? (from error.rs l.422)
//   -->  class/secure_types.baml:13
//    | 
// 12 |   j ((char, int[][], (bool | string[][])) | double[][][][], (float, int)[])
// 13 |   k map<string[], (int | long[])> | map<float[][], double[][]>
//    | 
// error: Type `AlphaNumeric_123_456_789` does not exist.
//   -->  class/secure_types.baml:14
//    | 
// 13 |   k map<string[], (int | long[])> | map<float[][], double[][]>
// 14 |   l AlphaNumeric_123_456_789 | (int, bool?) | char[]
//    | 
// error: Type `char` does not exist. Did you mean one of these: `int`, `float`, `bool`, `string`, `ComplexTypes`? (error.rs l.499)
//   -->  class/secure_types.baml:14
//    | 
// 13 |   k map<string[], (int | long[])> | map<float[][], double[][]>
// 14 |   l AlphaNumeric_123_456_789 | (int, bool?) | char[]
//    | 
// error: Type AlphaNumeric_123_456_789 not found. (from error.rs l.422)
//   -->  class/secure_types.baml:14
//    | 
// 13 |   k map<string[], (int | long[])> | map<float[][], double[][]>
// 14 |   l AlphaNumeric_123_456_789 | (int, bool?) | char[]
//    | 
// error: Type char not found. Did you mean one of these: `int`, `float`, `bool`? (from error.rs l.422)
//   -->  class/secure_types.baml:14
//    | 
// 13 |   k map<string[], (int | long[])> | map<float[][], double[][]>
// 14 |   l AlphaNumeric_123_456_789 | (int, bool?) | char[]
//    | 
// error: Type `tuple_1` does not exist. Did you mean one of these: `float`, `bool`, `string`, `int`, `ComplexTypes`? (error.rs l.499)
//   -->  class/secure_types.baml:15
//    | 
// 14 |   l AlphaNumeric_123_456_789 | (int, bool?) | char[]
// 15 |   m (tuple_1, tuple_2 | tuple_3, (tuple_4, tuple_5))[]
//    | 
// error: Type `tuple_2` does not exist. Did you mean one of these: `float`, `bool`, `string`, `int`, `ComplexTypes`? (error.rs l.499)
//   -->  class/secure_types.baml:15
//    | 
// 14 |   l AlphaNumeric_123_456_789 | (int, bool?) | char[]
// 15 |   m (tuple_1, tuple_2 | tuple_3, (tuple_4, tuple_5))[]
//    | 
// error: Type `tuple_3` does not exist. Did you mean one of these: `float`, `bool`, `string`, `int`, `ComplexTypes`? (error.rs l.499)
//   -->  class/secure_types.baml:15
//    | 
// 14 |   l AlphaNumeric_123_456_789 | (int, bool?) | char[]
// 15 |   m (tuple_1, tuple_2 | tuple_3, (tuple_4, tuple_5))[]
//    | 
// error: Type `tuple_4` does not exist. Did you mean one of these: `float`, `bool`, `string`, `int`, `ComplexTypes`? (error.rs l.499)
//   -->  class/secure_types.baml:15
//    | 
// 14 |   l AlphaNumeric_123_456_789 | (int, bool?) | char[]
// 15 |   m (tuple_1, tuple_2 | tuple_3, (tuple_4, tuple_5))[]
//    | 
// error: Type `tuple_5` does not exist. Did you mean one of these: `float`, `bool`, `string`, `int`, `ComplexTypes`? (error.rs l.499)
//   -->  class/secure_types.baml:15
//    | 
// 14 |   l AlphaNumeric_123_456_789 | (int, bool?) | char[]
// 15 |   m (tuple_1, tuple_2 | tuple_3, (tuple_4, tuple_5))[]
//    | 
// error: Type tuple_1 not found. Did you mean one of these: `null`, `float`, `bool`? (from error.rs l.422)
//   -->  class/secure_types.baml:15
//    | 
// 14 |   l AlphaNumeric_123_456_789 | (int, bool?) | char[]
// 15 |   m (tuple_1, tuple_2 | tuple_3, (tuple_4, tuple_5))[]
//    | 
// error: Type tuple_2 not found. Did you mean one of these: `null`, `float`, `bool`? (from error.rs l.422)
//   -->  class/secure_types.baml:15
//    | 
// 14 |   l AlphaNumeric_123_456_789 | (int, bool?) | char[]
// 15 |   m (tuple_1, tuple_2 | tuple_3, (tuple_4, tuple_5))[]
//    | 
// error: Type tuple_3 not found. Did you mean one of these: `null`, `float`, `bool`? (from error.rs l.422)
//   -->  class/secure_types.baml:15
//    | 
// 14 |   l AlphaNumeric_123_456_789 | (int, bool?) | char[]
// 15 |   m (tuple_1, tuple_2 | tuple_3, (tuple_4, tuple_5))[]
//    | 
// error: Type tuple_4 not found. Did you mean one of these: `null`, `float`, `bool`? (from error.rs l.422)
//   -->  class/secure_types.baml:15
//    | 
// 14 |   l AlphaNumeric_123_456_789 | (int, bool?) | char[]
// 15 |   m (tuple_1, tuple_2 | tuple_3, (tuple_4, tuple_5))[]
//    | 
// error: Type tuple_5 not found. Did you mean one of these: `null`, `float`, `bool`? (from error.rs l.422)
//   -->  class/secure_types.baml:15
//    | 
// 14 |   l AlphaNumeric_123_456_789 | (int, bool?) | char[]
// 15 |   m (tuple_1, tuple_2 | tuple_3, (tuple_4, tuple_5))[]
//    | 
// error: Type `another_key` does not exist. Did you mean one of these: `string`, `int`, `ComplexTypes`, `float`, `bool`? (error.rs l.499)
//   -->  class/secure_types.baml:16
//    | 
// 15 |   m (tuple_1, tuple_2 | tuple_3, (tuple_4, tuple_5))[]
// 16 |   n map<complex_key_type[], map<another_key, (int | string[])>>
//    | 
// error: Type `complex_key_type` does not exist. Did you mean one of these: `ComplexTypes`, `float`, `bool`, `int`, `string`? (error.rs l.499)
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
// error: Error validating: Maps may only have strings as keys
//   -->  class/secure_types.baml:16
//    | 
// 15 |   m (tuple_1, tuple_2 | tuple_3, (tuple_4, tuple_5))[]
// 16 |   n map<complex_key_type[], map<another_key, (int | string[])>>
//    | 
// error: Type `double` does not exist. Did you mean one of these: `bool`, `string`, `int`, `float`, `ComplexTypes`? (error.rs l.499)
//   -->  class/secure_types.baml:17
//    | 
// 16 |   n map<complex_key_type[], map<another_key, (int | string[])>>
// 17 |   o (((int | string) | bool[]), (float, double) | long_long_identifier_123)
//    | 
// error: Type `long_long_identifier_123` does not exist. Did you mean `ComplexTypes`?
//   -->  class/secure_types.baml:17
//    | 
// 16 |   n map<complex_key_type[], map<another_key, (int | string[])>>
// 17 |   o (((int | string) | bool[]), (float, double) | long_long_identifier_123)
//    | 
// error: Type double not found. Did you mean one of these: `bool`, `null`, `image`? (from error.rs l.422)
//   -->  class/secure_types.baml:17
//    | 
// 16 |   n map<complex_key_type[], map<another_key, (int | string[])>>
// 17 |   o (((int | string) | bool[]), (float, double) | long_long_identifier_123)
//    | 
// error: Type long_long_identifier_123 not found. Did you mean `ComplexTypes`? (from error.rs l.422)
//   -->  class/secure_types.baml:17
//    | 
// 16 |   n map<complex_key_type[], map<another_key, (int | string[])>>
// 17 |   o (((int | string) | bool[]), (float, double) | long_long_identifier_123)
//    | 
