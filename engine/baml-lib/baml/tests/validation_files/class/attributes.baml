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