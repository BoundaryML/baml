class TestClassAlias {
  we
  key string @alias("key-dash") 
  @description(#"
     This is a description for key
     af asdf
   "#)
  key2 string @alias("key21") @alias("key22")
  key3 string @alias("key with space")
  key4 string @alias("key.with.punctuation/123")
}

//----
//- diagnostics
// Error: field 'we' is missing a type annotation
//    ╭─[ 0:2:3 ]
//    │
//  2 │   we
//    │   ─┬  
//    │    ╰── field 'we' is missing a type annotation
//    │ 
//    │ Note: Error code: E0010
// ───╯
// Error: Attribute '@alias' can only be defined once on field 'key2' in class 'TestClassAlias'
//    ╭─[ 0:8:32 ]
//    │
//  8 │   key2 string @alias("key21") @alias("key22")
//    │                ──┬──           ──┬──  
//    │                  ╰──────────────────── first definition here
//    │                                  │    
//    │                                  ╰──── duplicate attribute
//    │ 
//    │ Note: Error code: E0014
// ───╯
