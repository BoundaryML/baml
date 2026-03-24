class TestClassAlias {
  key string @alias("key-dash")
  @description(#"
     This is a description for key
     af asdf
   "#)
  key2 string @alias("key21") @alias("key22")
  key3 string @alias("key with space")
  key4 string @alias("key.with.punctuation/123") @check(trivial, {{this == "Homo sapiens"}}) @check(regex_good, {{this|regex_match("Homo")}})
  key5 int @assert(positive, {{this > 0}}) @assert(small, {{this < 100}})
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
// Error: unreachable arm
//      ╭─[ llm_types.baml:159:17 ]
//      │
//  159 │             _ => { continue; }
//      │                 ───────┬──────  
//      │                        ╰──────── unreachable arm
//      │ 
//      │ Note: Error code: E0001
// ─────╯
// Error: unreachable arm
//      ╭─[ llm_types.baml:189:17 ]
//      │
//  189 │ ╭─▶             _ => {
//      ┆ ┆   
//  194 │ ├─▶             }
//      │ │                   
//      │ ╰─────────────────── unreachable arm
//      │     
//      │     Note: Error code: E0001
// ─────╯
// Error: unreachable arm
//      ╭─[ llm_types.baml:208:17 ]
//      │
//  208 │             _ => { continue; }
//      │                 ───────┬──────  
//      │                        ╰──────── unreachable arm
//      │ 
//      │ Note: Error code: E0001
// ─────╯
