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
// E0002
//
//   × unresolved type: apple_pie
//    ╭─[class_secure_types.baml:3:5]
//  3 │   a map<string[], (int | bool[]) | apple_pie[][]>
//    ·     ─────────────────────────────────────────────
//    ╰────
// E0067
//
//   × map keys must be `string`; got `string[]`. Declare the map as `map<string, V>`; convert non-string keys with `.to_string()` before `.set()` or `.get()`
//    ╭─[class_secure_types.baml:3:5]
//  3 │   a map<string[], (int | bool[]) | apple_pie[][]>
//    ·     ─────────────────────────────────────────────
//    ╰────
// E0067
//
//   × map keys must be `string`; got `bool`. Declare the map as `map<string, V>`; convert non-string keys with `.to_string()` before `.set()` or `.get()`
//    ╭─[class_secure_types.baml:4:5]
//  4 │   b (int, map<bool, string?>, (char | float)[][] | long_word_123.foobar[])
//    ·     ──────────────────────────────────────────────────────────────────────
//    ╰────
// E0002
//
//   × unresolved type: char
//    ╭─[class_secure_types.baml:4:5]
//  4 │   b (int, map<bool, string?>, (char | float)[][] | long_word_123.foobar[])
//    ·     ──────────────────────────────────────────────────────────────────────
//    ╰────
// E0002
//
//   × unresolved type: long_word_123.foobar
//    ╭─[class_secure_types.baml:4:5]
//  4 │   b (int, map<bool, string?>, (char | float)[][] | long_word_123.foobar[])
//    ·     ──────────────────────────────────────────────────────────────────────
//    ╰────
// E0002
//
//   × unresolved type: apple123_456_pie
//    ╭─[class_secure_types.baml:5:5]
//  5 │   c apple123_456_pie | (stringer, bool[], (int | char))[]
//    ·     ─────────────────────────────────────────────────────
//    ╰────
// E0002
//
//   × unresolved type: stringer
//    ╭─[class_secure_types.baml:5:5]
//  5 │   c apple123_456_pie | (stringer, bool[], (int | char))[]
//    ·     ─────────────────────────────────────────────────────
//    ╰────
// E0002
//
//   × unresolved type: char
//    ╭─[class_secure_types.baml:6:5]
//  6 │   d map<int[][], ((int | float) | char[])>
//    ·     ──────────────────────────────────────
//    ╰────
// E0067
//
//   × map keys must be `string`; got `int[][]`. Declare the map as `map<string, V>`; convert non-string keys with `.to_string()` before `.set()` or `.get()`
//    ╭─[class_secure_types.baml:6:5]
//  6 │   d map<int[][], ((int | float) | char[])>
//    ·     ──────────────────────────────────────
//    ╰────
// E0002
//
//   × unresolved type: long
//    ╭─[class_secure_types.baml:7:5]
//  7 │   e ((int, string | char) | ((float, double) | long[], bool)[][][])
//    ·     ───────────────────────────────────────────────────────────────
//    ╰────
// E0002
//
//   × unresolved type: VeryLongWord_With_123_Numbers
//    ╭─[class_secure_types.baml:8:5]
//  8 │   f VeryLongWord_With_123_Numbers[][][][]
//    ·     ─────────────────────────────────────
//    ╰────
// E0002
//
//   × unresolved type: tuple_inside_tuple
//    ╭─[class_secure_types.baml:9:5]
//  9 │   g (int, (float, char, bool), string[]) | tuple_inside_tuple[]
//    ·     ───────────────────────────────────────────────────────────
//    ╰────
// E0002
//
//   × unresolved type: char
//     ╭─[class_secure_types.baml:10:5]
//  10 │   h (((int | string)[]) | map<bool[][], char[]>)
//     ·     ────────────────────────────────────────────
//     ╰────
// E0067
//
//   × map keys must be `string`; got `bool[][]`. Declare the map as `map<string, V>`; convert non-string keys with `.to_string()` before `.set()` or `.get()`
//     ╭─[class_secure_types.baml:10:5]
//  10 │   h (((int | string)[]) | map<bool[][], char[]>)
//     ·     ────────────────────────────────────────────
//     ╰────
// E0002
//
//   × unresolved type: apple
//     ╭─[class_secure_types.baml:11:5]
//  11 │   i (apple, banana | cherry | date_fruit | eggplant_vegetable)[]
//     ·     ────────────────────────────────────────────────────────────
//     ╰────
// E0002
//
//   × unresolved type: banana
//     ╭─[class_secure_types.baml:11:5]
//  11 │   i (apple, banana | cherry | date_fruit | eggplant_vegetable)[]
//     ·     ────────────────────────────────────────────────────────────
//     ╰────
// E0002
//
//   × unresolved type: cherry
//     ╭─[class_secure_types.baml:11:5]
//  11 │   i (apple, banana | cherry | date_fruit | eggplant_vegetable)[]
//     ·     ────────────────────────────────────────────────────────────
//     ╰────
// E0002
//
//   × unresolved type: date_fruit
//     ╭─[class_secure_types.baml:11:5]
//  11 │   i (apple, banana | cherry | date_fruit | eggplant_vegetable)[]
//     ·     ────────────────────────────────────────────────────────────
//     ╰────
// E0002
//
//   × unresolved type: eggplant_vegetable
//     ╭─[class_secure_types.baml:11:5]
//  11 │   i (apple, banana | cherry | date_fruit | eggplant_vegetable)[]
//     ·     ────────────────────────────────────────────────────────────
//     ╰────
// E0002
//
//   × unresolved type: char
//     ╭─[class_secure_types.baml:12:5]
//  12 │   j ((char, int[][], (bool | string[][])) | double[][][][], (float, int)[])
//     ·     ───────────────────────────────────────────────────────────────────────
//     ╰────
// E0002
//
//   × unresolved type: double
//     ╭─[class_secure_types.baml:12:5]
//  12 │   j ((char, int[][], (bool | string[][])) | double[][][][], (float, int)[])
//     ·     ───────────────────────────────────────────────────────────────────────
//     ╰────
// E0002
//
//   × unresolved type: long
//     ╭─[class_secure_types.baml:13:5]
//  13 │   k map<string[], (int | long[])> | map<float[][], double[][]>
//     ·     ──────────────────────────────────────────────────────────
//     ╰────
// E0067
//
//   × map keys must be `string`; got `string[]`. Declare the map as `map<string, V>`; convert non-string keys with `.to_string()` before `.set()` or `.get()`
//     ╭─[class_secure_types.baml:13:5]
//  13 │   k map<string[], (int | long[])> | map<float[][], double[][]>
//     ·     ──────────────────────────────────────────────────────────
//     ╰────
// E0002
//
//   × unresolved type: double
//     ╭─[class_secure_types.baml:13:5]
//  13 │   k map<string[], (int | long[])> | map<float[][], double[][]>
//     ·     ──────────────────────────────────────────────────────────
//     ╰────
// E0067
//
//   × map keys must be `string`; got `float[][]`. Declare the map as `map<string, V>`; convert non-string keys with `.to_string()` before `.set()` or `.get()`
//     ╭─[class_secure_types.baml:13:5]
//  13 │   k map<string[], (int | long[])> | map<float[][], double[][]>
//     ·     ──────────────────────────────────────────────────────────
//     ╰────
// E0002
//
//   × unresolved type: AlphaNumeric_123_456_789
//     ╭─[class_secure_types.baml:14:5]
//  14 │   l AlphaNumeric_123_456_789 | (int, bool?) | char[]
//     ·     ────────────────────────────────────────────────
//     ╰────
// E0002
//
//   × unresolved type: char
//     ╭─[class_secure_types.baml:14:5]
//  14 │   l AlphaNumeric_123_456_789 | (int, bool?) | char[]
//     ·     ────────────────────────────────────────────────
//     ╰────
// E0002
//
//   × unresolved type: tuple_1
//     ╭─[class_secure_types.baml:15:5]
//  15 │   m (tuple_1, tuple_2 | tuple_3, (tuple_4, tuple_5))[]
//     ·     ──────────────────────────────────────────────────
//     ╰────
// E0002
//
//   × unresolved type: tuple_2
//     ╭─[class_secure_types.baml:15:5]
//  15 │   m (tuple_1, tuple_2 | tuple_3, (tuple_4, tuple_5))[]
//     ·     ──────────────────────────────────────────────────
//     ╰────
// E0002
//
//   × unresolved type: tuple_3
//     ╭─[class_secure_types.baml:15:5]
//  15 │   m (tuple_1, tuple_2 | tuple_3, (tuple_4, tuple_5))[]
//     ·     ──────────────────────────────────────────────────
//     ╰────
// E0002
//
//   × unresolved type: tuple_4
//     ╭─[class_secure_types.baml:15:5]
//  15 │   m (tuple_1, tuple_2 | tuple_3, (tuple_4, tuple_5))[]
//     ·     ──────────────────────────────────────────────────
//     ╰────
// E0002
//
//   × unresolved type: tuple_5
//     ╭─[class_secure_types.baml:15:5]
//  15 │   m (tuple_1, tuple_2 | tuple_3, (tuple_4, tuple_5))[]
//     ·     ──────────────────────────────────────────────────
//     ╰────
// E0002
//
//   × unresolved type: complex_key_type
//     ╭─[class_secure_types.baml:16:5]
//  16 │   n map<complex_key_type[], map<another_key, (int | string[])>>
//     ·     ───────────────────────────────────────────────────────────
//     ╰────
// E0002
//
//   × unresolved type: another_key
//     ╭─[class_secure_types.baml:16:5]
//  16 │   n map<complex_key_type[], map<another_key, (int | string[])>>
//     ·     ───────────────────────────────────────────────────────────
//     ╰────
// E0067
//
//   × map keys must be `string`; got `!error[]`. Declare the map as `map<string, V>`; convert non-string keys with `.to_string()` before `.set()` or `.get()`
//     ╭─[class_secure_types.baml:16:5]
//  16 │   n map<complex_key_type[], map<another_key, (int | string[])>>
//     ·     ───────────────────────────────────────────────────────────
//     ╰────
// E0002
//
//   × unresolved type: long_long_identifier_123
//     ╭─[class_secure_types.baml:17:5]
//  17 │   o (((int | string) | bool[]), (float, double) | long_long_identifier_123)
//     ·     ───────────────────────────────────────────────────────────────────────
//     ╰────
