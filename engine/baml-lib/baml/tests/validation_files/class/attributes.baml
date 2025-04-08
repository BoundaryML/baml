class TestClassAlias {
  key string @alias("key-dash") 
  @description(#"
     This is a description for key
     af asdf
   "#)
  key2 string @alias("key21")
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
