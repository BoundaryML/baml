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
//    ╭─[class_attributes.baml:7:31]
//  7 │   key2 string @alias("key21") @alias("key22")
//    ·               ───────┬─────── ───────┬───────
//    ·                      │               ╰── duplicate `@alias` — only the last takes effect
//    ·                      ╰── `@alias` first applied here
//    ╰────
// E0014
//
//   × duplicate attribute `@description`
//     ╭─[class_attributes.baml:20:3]
//  20 │   @@description("A test")
//     ·   ───────────┬───────────
//     ·              ╰── `@description` first applied here
//     ╰────
//     ╭─[class_attributes.baml:22:3]
//  22 │   @@description("A second description")
//     ·   ──────────────────┬──────────────────
//     ·                     ╰── duplicate `@description` — only the last takes effect
//     ╰────
