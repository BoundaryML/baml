client<llm> MyClient {
  provider baml-openai-chat
  // dictionary test
  options {
    // no commas
    max_tokens 100
    array_key_with_quoted_unquoted [ "abc", "abc", "ccc" ]
    // separate by commas
    key2 "some random value", hello "world", thing "hello"
    block_string #"hello there my frien
                d"#
    inline_raw_string #"inline "raw" string,,"#
    keyStringNoSpacesWorksWithoutQuotes NoQuotes!!123
    unquoted_string_whitespace hello world
    "string key" "value with spaces"
    filePathString ../this-is-also-a-string
    booleanValue true
    stringKey "true"
    array1 [
      "one"
      "two"
    ]
    // a comment and new empty_lines

    array2 [ "one", "two" ]
    array3Numbers [ 1, 2, 3 ]
    nestedKey {
      key "value"
      key2 "value2"
      nestedArray [ "yes", "queen"]
      number 10
    }
    oneMoreValueToTestBrokenness "value"
    myInteger 100
    block_string #"
      hello there my friend
      #{//a comment in a prompt}
    "#
  }
}

// error: Error validating function "MyClient": Invalid function syntax. Valid function syntax is
// ```
// function MyClient(param1: String, param2: String) -> ReturnType {
//     client SomeClient
//     prompt #"..."#
// }
// ```
//   -->  dictionary/valid_dictionary.baml:1
//    | 
//    | 
//  1 | client<llm> MyClient {
//  2 |   provider baml-openai-chat
//  3 |   // dictionary test
//  4 |   options {
//  5 |     // no commas
//  6 |     max_tokens 100
//  7 |     array_key_with_quoted_unquoted [ "abc", "abc", "ccc" ]
//  8 |     // separate by commas
//  9 |     key2 "some random value", hello "world", thing "hello"
// 10 |     block_string #"hello there my frien
// 11 |                 d"#
// 12 |     inline_raw_string #"inline "raw" string,,"#
// 13 |     keyStringNoSpacesWorksWithoutQuotes NoQuotes!!123
// 14 |     unquoted_string_whitespace hello world
// 15 |     "string key" "value with spaces"
// 16 |     filePathString ../this-is-also-a-string
// 17 |     booleanValue true
// 18 |     stringKey "true"
// 19 |     array1 [
// 20 |       "one"
// 21 |       "two"
// 22 |     ]
// 23 |     // a comment and new empty_lines
// 24 | 
// 25 |     array2 [ "one", "two" ]
// 26 |     array3Numbers [ 1, 2, 3 ]
// 27 |     nestedKey {
// 28 |       key "value"
// 29 |       key2 "value2"
// 30 |       nestedArray [ "yes", "queen"]
// 31 |       number 10
// 32 |     }
// 33 |     oneMoreValueToTestBrokenness "value"
// 34 |     myInteger 100
// 35 |     block_string #"
// 36 |       hello there my friend
// 37 |       #{//a comment in a prompt}
// 38 |     "#
// 39 |   }
// 40 | }
//    | 
