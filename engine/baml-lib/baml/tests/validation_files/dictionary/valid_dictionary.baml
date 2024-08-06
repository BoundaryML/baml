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

// error: Error validating Client "MyClient": This field declaration is invalid. It is either missing a name or a type.
//   -->  dictionary/valid_dictionary.baml:4
//    | 
//  3 |   // dictionary test
//  4 |   options {
//    | 
// error: Error validating: This line is not a valid field or attribute definition.
//   -->  dictionary/valid_dictionary.baml:4
//    | 
//  3 |   // dictionary test
//  4 |   options {
//  5 |     // no commas
//    | 
// error: Error validating: This line is not a valid field or attribute definition.
//   -->  dictionary/valid_dictionary.baml:9
//    | 
//  8 |     // separate by commas
//  9 |     key2 "some random value", hello "world", thing "hello"
// 10 |     block_string #"hello there my frien
//    | 
// error: Error validating: This line is not a valid field or attribute definition.
//   -->  dictionary/valid_dictionary.baml:15
//    | 
// 14 |     unquoted_string_whitespace hello world
// 15 |     "string key" "value with spaces"
// 16 |     filePathString ../this-is-also-a-string
//    | 
// error: Error validating: This line is invalid. It does not start with any known Baml schema keyword.
//   -->  dictionary/valid_dictionary.baml:40
//    | 
// 39 |   }
// 40 | }
// 41 | 
//    | 
// error: Error validating: Unknown field `max_tokens` in client
//   -->  dictionary/valid_dictionary.baml:6
//    | 
//  5 |     // no commas
//  6 |     max_tokens 100
//    | 
// error: Error validating: Unknown field `array_key_with_quoted_unquoted` in client
//   -->  dictionary/valid_dictionary.baml:7
//    | 
//  6 |     max_tokens 100
//  7 |     array_key_with_quoted_unquoted [ "abc", "abc", "ccc" ]
//    | 
// error: Error validating: Unknown field `key2` in client
//   -->  dictionary/valid_dictionary.baml:9
//    | 
//  8 |     // separate by commas
//  9 |     key2 "some random value", hello "world", thing "hello"
//    | 
// error: Error validating: Unknown field `block_string` in client
//   -->  dictionary/valid_dictionary.baml:10
//    | 
//  9 |     key2 "some random value", hello "world", thing "hello"
// 10 |     block_string #"hello there my frien
// 11 |                 d"#
//    | 
// error: Error validating: Unknown field `inline_raw_string` in client
//   -->  dictionary/valid_dictionary.baml:12
//    | 
// 11 |                 d"#
// 12 |     inline_raw_string #"inline "raw" string,,"#
//    | 
// error: Error validating: Unknown field `keyStringNoSpacesWorksWithoutQuotes` in client
//   -->  dictionary/valid_dictionary.baml:13
//    | 
// 12 |     inline_raw_string #"inline "raw" string,,"#
// 13 |     keyStringNoSpacesWorksWithoutQuotes NoQuotes!!123
//    | 
// error: Error validating: Unknown field `unquoted_string_whitespace` in client
//   -->  dictionary/valid_dictionary.baml:14
//    | 
// 13 |     keyStringNoSpacesWorksWithoutQuotes NoQuotes!!123
// 14 |     unquoted_string_whitespace hello world
//    | 
// error: Error validating: Unknown field `filePathString` in client
//   -->  dictionary/valid_dictionary.baml:16
//    | 
// 15 |     "string key" "value with spaces"
// 16 |     filePathString ../this-is-also-a-string
//    | 
// error: Error validating: Unknown field `booleanValue` in client
//   -->  dictionary/valid_dictionary.baml:17
//    | 
// 16 |     filePathString ../this-is-also-a-string
// 17 |     booleanValue true
//    | 
// error: Error validating: Unknown field `stringKey` in client
//   -->  dictionary/valid_dictionary.baml:18
//    | 
// 17 |     booleanValue true
// 18 |     stringKey "true"
//    | 
// error: Error validating: Unknown field `array1` in client
//   -->  dictionary/valid_dictionary.baml:19
//    | 
// 18 |     stringKey "true"
// 19 |     array1 [
// 20 |       "one"
// 21 |       "two"
// 22 |     ]
//    | 
// error: Error validating: Unknown field `array2` in client
//   -->  dictionary/valid_dictionary.baml:25
//    | 
// 24 | 
// 25 |     array2 [ "one", "two" ]
//    | 
// error: Error validating: Unknown field `array3Numbers` in client
//   -->  dictionary/valid_dictionary.baml:26
//    | 
// 25 |     array2 [ "one", "two" ]
// 26 |     array3Numbers [ 1, 2, 3 ]
//    | 
// error: Error validating: Unknown field `nestedKey` in client
//   -->  dictionary/valid_dictionary.baml:27
//    | 
// 26 |     array3Numbers [ 1, 2, 3 ]
// 27 |     nestedKey {
// 28 |       key "value"
// 29 |       key2 "value2"
// 30 |       nestedArray [ "yes", "queen"]
// 31 |       number 10
// 32 |     }
//    | 
// error: Error validating: Unknown field `oneMoreValueToTestBrokenness` in client
//   -->  dictionary/valid_dictionary.baml:33
//    | 
// 32 |     }
// 33 |     oneMoreValueToTestBrokenness "value"
//    | 
// error: Error validating: Unknown field `myInteger` in client
//   -->  dictionary/valid_dictionary.baml:34
//    | 
// 33 |     oneMoreValueToTestBrokenness "value"
// 34 |     myInteger 100
//    | 
// error: Error validating: Unknown field `block_string` in client
//   -->  dictionary/valid_dictionary.baml:35
//    | 
// 34 |     myInteger 100
// 35 |     block_string #"
// 36 |       hello there my friend
// 37 |       #{//a comment in a prompt}
// 38 |     "#
//    | 
