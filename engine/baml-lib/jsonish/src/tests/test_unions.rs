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

const MAP_VS_CLASS: &str = r###"
class Page {
    object string @description("Always page")
    // children (Bookmark | Breadcrumb | BulletedListItem | Callout | Code | ColumnList | Divider | Embed | Equation | File | Heading1 | Heading2 | Heading3 | ImageFile | NumberedListItem | Paragraph | PDF | Quote | Table | TableOfContents | ToDo | Toggle | Video)[] @description("Some blocks of the page")
    children (Breadcrumb | ColumnList)[] @description("Some blocks of the page")
}

// ------------ Breadcrumb ----------------
class Breadcrumb {
  type string @description("Always breadcrumb")
  breadcrumb map<string, string> @description("Always empty map")
}

/// ------------ Column List ----------------
class ColumnList {
  type string @description("Always column_list")
  column_list ColumnListBody @description("Column list block with columns")
}

class ColumnListBody {
  children Column[] @description("The columns in the column list. Max length is 5")
}

class Column {
  type string @description("Always column")
  column ColumnBody @description("Always empty map for columns")
}

class ColumnBody {
  // children (Bookmark | Breadcrumb | BulletedListItem | Callout | Code | Divider | Embed | Equation | File | Heading1 | Heading2 | Heading3 | ImageFile | NumberedListItem | Paragraph | PDF | Quote | Table | TableOfContents | ToDo | Toggle | Video)[] @description("Content of the column. Can contain any block type. Min length is 1")
  children (Breadcrumb | Paragraph)[] 
}

/// ------------ Other blocks ----------------
/// 
class Paragraph {
  type string @description("Always paragraph")
  paragraph string
}
"###;

test_deserializer!(
  prefer_class_to_map,
  MAP_VS_CLASS,
  r#"```json
  {
    "object": "page",
    "children": [
      {
        "type": "column_list",
        "column_list": {
          "children": [
            {
              "type": "column",
              "column": {
                "children": [
                  {
                    "untyped": "lorem ipsum",
                  },
                  {
                    "untyped": "lorem ipsum",
                  },
                  {
                    "untyped": "lorem ipsum",
                  },
                  {
                    "type": "paragraph",
                    "paragraph":  "J.R.R. Tolkien",
                  },
                  {
                    "type": "to_do",
                    "to_do": {
                    "untyped": "dolor sit amet",
                      "rich_text": [
                        {
                          "type": "text",
                          "text": {
                            "content": "Read again"
                          }
                        }
                      ],
                      "checked": false
                    }
                  }
                ]
              }
            }
          ]
        }
      }
    ]
  }
  ```"#,
  FieldType::class("Page"),
  {
    "children": [
      {
        "type": "column_list",
        "column_list": {
          "children": [
            {
              "type": "column",
              "column": {
                "children": [
                  {
                    "type": "paragraph",
                    "paragraph":  "J.R.R. Tolkien",
                  },
                ]
              }
            }
          ]
        }
      }
    ]
  }
);

test_deserializer!(
  array_of_union_should_drop_unknown,
  MAP_VS_CLASS,
  r#"```json
  {
    "children": [
      {
        "untyped": "lorem ipsum",
      },
      {
        "untyped": "lorem ipsum",
      },
      {
        "untyped": "lorem ipsum",
      },
      {
        "untyped": "lorem ipsum",
      },
      {
        "type": "paragraph",
        "paragraph":  "J.R.R. Tolkien",
      },
      {
        "type": "to_do",
        "to_do": {
        "untyped": "dolor sit amet",
          "rich_text": [
            {
              "type": "text",
              "text": {
                "content": "Read again"
              }
            }
          ],
          "checked": false
        }
      }
    ]
  }
  ```"#,
  FieldType::class("ColumnBody"),
  {
    "children": [
      {
        "type": "paragraph",
        "paragraph":  "J.R.R. Tolkien",
      },
      {
        "type": "to_do",
        "breadcrumb": {},
      }
    ]
  }
);
