class TestClassAlias {
  we
  key string @alias("key-dash")
  @description("
     This is a description for key
     af asdf
   ")
  key2 string @alias("key21") @alias("key22")
  key3 string @alias("key with space")
  key4 string @alias("key.with.punctuation/123")
}

//----
//- diagnostics
// E0010
//
//   × invalid syntax
//    ╭─[class_partial_class.baml:2:3]
//  2 │   we
//    ·   ─┬
//    ·    ╰── field 'we' is missing a type annotation
//    ╰────
// E0014
//
//   × duplicate attribute `@alias`
//    ╭─[class_partial_class.baml:8:31]
//  8 │   key2 string @alias("key21") @alias("key22")
//    ·               ───────┬─────── ───────┬───────
//    ·                      │               ╰── duplicate `@alias` — only the last takes effect
//    ·                      ╰── `@alias` first applied here
//    ╰────
