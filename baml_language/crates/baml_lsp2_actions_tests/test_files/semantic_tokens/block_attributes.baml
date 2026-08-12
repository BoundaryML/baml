class TestClassAlias {
  key string @alias("key-dash")
  @description(#"
     This is a description for key
     af asdf
   "#)
  key2 string @alias("key21") @alias("key22")
  key3 string @alias("key with space")
  key4 string @alias("key.with.punctuation/123")
}

class HasTwoBlockAttributes {
  name string
  @@description("A test")
  @@alias("AKAHasTwo")
}

class HasThreeBlockAttributes {
  name string
  @@description("A test")
  @@alias("AKAHasTwo")
  @@description("A second description")
}

// error: Attribute "@description" can only be defined once.
//   -->  class/attributes.baml:20
//    |
// 19 |   name string
// 20 |   @@description("A test")
//    |
// error: Attribute "@description" can only be defined once.
//   -->  class/attributes.baml:22
//    |
// 21 |   @@alias("AKAHasTwo")
// 22 |   @@description("A second description")
//    |
// error: Attribute "@description" can only be defined once.
//   -->  class/attributes.baml:20
//    |
// 19 |   name string
// 20 |   @@description("A test")
//    |

//----
//- diagnostics
// E0014
//
//   × duplicate attribute `@alias`
//    ╭─[block_attributes.baml:7:31]
//  7 │   key2 string @alias("key21") @alias("key22")
//    ·               ───────┬─────── ───────┬───────
//    ·                      │               ╰── duplicate `@alias` — only the last takes effect
//    ·                      ╰── `@alias` first applied here
//    ╰────
// E0014
//
//   × duplicate attribute `@description`
//     ╭─[block_attributes.baml:20:3]
//  20 │   @@description("A test")
//     ·   ───────────┬───────────
//     ·              ╰── `@description` first applied here
//     ╰────
//     ╭─[block_attributes.baml:22:3]
//  22 │   @@description("A second description")
//     ·   ──────────────────┬──────────────────
//     ·                     ╰── duplicate `@description` — only the last takes effect
//     ╰────
//
//- semantic_tokens
// block_attributes.baml:1:1 (keyword) len=5 "class"
// block_attributes.baml:1:7 (class) [declaration] len=14 "TestClassAlias"
// block_attributes.baml:2:3 (property) [declaration] len=3 "key"
// block_attributes.baml:2:7 (type) [defaultLibrary] len=6 "string"
// block_attributes.baml:2:14 (decorator) len=1 "@"
// block_attributes.baml:2:15 (decorator) len=5 "alias"
// block_attributes.baml:2:21 (string) len=10 "\"key-dash\""
// block_attributes.baml:3:3 (decorator) len=1 "@"
// block_attributes.baml:3:4 (decorator) len=11 "description"
// block_attributes.baml:3:16 (string) len=1 "#"
// block_attributes.baml:3:17 (string) len=1 "\""
// block_attributes.baml:4:6 (string) len=4 "This"
// block_attributes.baml:4:11 (string) len=2 "is"
// block_attributes.baml:4:14 (string) len=1 "a"
// block_attributes.baml:4:16 (string) len=11 "description"
// block_attributes.baml:4:28 (string) len=3 "for"
// block_attributes.baml:4:32 (string) len=3 "key"
// block_attributes.baml:5:6 (string) len=2 "af"
// block_attributes.baml:5:9 (string) len=4 "asdf"
// block_attributes.baml:6:4 (string) len=1 "\""
// block_attributes.baml:6:5 (string) len=1 "#"
// block_attributes.baml:7:3 (property) [declaration] len=4 "key2"
// block_attributes.baml:7:8 (type) [defaultLibrary] len=6 "string"
// block_attributes.baml:7:15 (decorator) len=1 "@"
// block_attributes.baml:7:16 (decorator) len=5 "alias"
// block_attributes.baml:7:22 (string) len=7 "\"key21\""
// block_attributes.baml:7:31 (decorator) len=1 "@"
// block_attributes.baml:7:32 (decorator) len=5 "alias"
// block_attributes.baml:7:38 (string) len=7 "\"key22\""
// block_attributes.baml:8:3 (property) [declaration] len=4 "key3"
// block_attributes.baml:8:8 (type) [defaultLibrary] len=6 "string"
// block_attributes.baml:8:15 (decorator) len=1 "@"
// block_attributes.baml:8:16 (decorator) len=5 "alias"
// block_attributes.baml:8:22 (string) len=16 "\"key with space\""
// block_attributes.baml:9:3 (property) [declaration] len=4 "key4"
// block_attributes.baml:9:8 (type) [defaultLibrary] len=6 "string"
// block_attributes.baml:9:15 (decorator) len=1 "@"
// block_attributes.baml:9:16 (decorator) len=5 "alias"
// block_attributes.baml:9:22 (string) len=26 "\"key.with.punctuation/123\""
// block_attributes.baml:12:1 (keyword) len=5 "class"
// block_attributes.baml:12:7 (class) [declaration] len=21 "HasTwoBlockAttributes"
// block_attributes.baml:13:3 (property) [declaration] len=4 "name"
// block_attributes.baml:13:8 (type) [defaultLibrary] len=6 "string"
// block_attributes.baml:14:3 (decorator) len=2 "@@"
// block_attributes.baml:14:5 (decorator) len=11 "description"
// block_attributes.baml:14:17 (string) len=8 "\"A test\""
// block_attributes.baml:15:3 (decorator) len=2 "@@"
// block_attributes.baml:15:5 (decorator) len=5 "alias"
// block_attributes.baml:15:11 (string) len=11 "\"AKAHasTwo\""
// block_attributes.baml:18:1 (keyword) len=5 "class"
// block_attributes.baml:18:7 (class) [declaration] len=23 "HasThreeBlockAttributes"
// block_attributes.baml:19:3 (property) [declaration] len=4 "name"
// block_attributes.baml:19:8 (type) [defaultLibrary] len=6 "string"
// block_attributes.baml:20:3 (decorator) len=2 "@@"
// block_attributes.baml:20:5 (decorator) len=11 "description"
// block_attributes.baml:20:17 (string) len=8 "\"A test\""
// block_attributes.baml:21:3 (decorator) len=2 "@@"
// block_attributes.baml:21:5 (decorator) len=5 "alias"
// block_attributes.baml:21:11 (string) len=11 "\"AKAHasTwo\""
// block_attributes.baml:22:3 (decorator) len=2 "@@"
// block_attributes.baml:22:5 (decorator) len=11 "description"
// block_attributes.baml:22:17 (string) len=22 "\"A second description\""
// block_attributes.baml:25:1 (comment) len=60 "// error: Attribute \"@description\" can only be defined once."
// block_attributes.baml:26:1 (comment) len=34 "//   -->  class/attributes.baml:20"
// block_attributes.baml:27:1 (comment) len=7 "//    |"
// block_attributes.baml:28:1 (comment) len=21 "// 19 |   name string"
// block_attributes.baml:29:1 (comment) len=33 "// 20 |   @@description(\"A test\")"
// block_attributes.baml:30:1 (comment) len=7 "//    |"
// block_attributes.baml:31:1 (comment) len=60 "// error: Attribute \"@description\" can only be defined once."
// block_attributes.baml:32:1 (comment) len=34 "//   -->  class/attributes.baml:22"
// block_attributes.baml:33:1 (comment) len=7 "//    |"
// block_attributes.baml:34:1 (comment) len=30 "// 21 |   @@alias(\"AKAHasTwo\")"
// block_attributes.baml:35:1 (comment) len=47 "// 22 |   @@description(\"A second description\")"
// block_attributes.baml:36:1 (comment) len=7 "//    |"
// block_attributes.baml:37:1 (comment) len=60 "// error: Attribute \"@description\" can only be defined once."
// block_attributes.baml:38:1 (comment) len=34 "//   -->  class/attributes.baml:20"
// block_attributes.baml:39:1 (comment) len=7 "//    |"
// block_attributes.baml:40:1 (comment) len=21 "// 19 |   name string"
// block_attributes.baml:41:1 (comment) len=33 "// 20 |   @@description(\"A test\")"
// block_attributes.baml:42:1 (comment) len=7 "//    |"
