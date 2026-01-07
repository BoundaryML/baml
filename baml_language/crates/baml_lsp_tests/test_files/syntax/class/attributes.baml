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
// Error: Attribute `@alias` can only be defined once on field `key2` in class `TestClassAlias`
//    ╭─[ class_attributes.baml:7:32 ]
//    │
//  7 │   key2 string @alias("key21") @alias("key22")
//    │                ──┬──           ──┬──  
//    │                  ╰──────────────────── first definition here
//    │                                  │    
//    │                                  ╰──── duplicate attribute
//    │ 
//    │ Note: Error code: E0014
// ───╯
// Error: Attribute `@@description` can only be defined once on class `HasThreeBlockAttributes`
//     ╭─[ class_attributes.baml:22:5 ]
//     │
//  22 │   @@description("A second description")
//     │     ─────┬─────  
//     │          ╰─────── duplicate attribute
//     │
//     ├─[ class_attributes.baml:22:5 ]
//     │
//  20 │   @@description("A test")
//     │     ─────┬─────  
//     │          ╰─────── first definition here
//     │ 
//     │ Note: Error code: E0014
// ────╯
