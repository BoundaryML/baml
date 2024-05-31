use super::*;

//
const FOO_FILE: &str = r#"
class Foo {
  hi string[]
}

class Bar {
  foo string
}
"#;

test_deserializer!(
  test_union,
  FOO_FILE,
  r#"{"hi": ["a", "b"]}"#,
  FieldType::union(vec![FieldType::class("Foo"), FieldType::class("Bar")]),
  {"hi": ["a", "b"]}
);

const SPUR_FILE: &str = r###"
enum CatA {
  A
}

enum CatB {
  C
  D
}

class CatAPicker {
  cat CatA
}

class CatBPicker {
  cat CatB
  item int
}

enum CatC {
  E
  F 
  G 
  H 
  I
}

class CatCPicker {
  cat CatC
  item  int | string | null
  data int?
}
"###;

test_deserializer!(
  test_union2,
  SPUR_FILE,
  r#"```json
  {
    "cat": "E",
    "item": "28558C",
    "data": null
  }
  ```"#,
  FieldType::union(vec![FieldType::class("CatAPicker"), FieldType::class("CatBPicker"), FieldType::class("CatCPicker")]),
  {
    "cat": "E",
    "item": "28558C",
    "data": null
  }
);
