client<llm> MyClient {
  provider openai
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
    "string key" "value with spaces"
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
    // array of objects with nested config blocks (no colons)
    tools [
      {
        server_label "parallel_web_search"
        server_url "https://example.com"
        headers {
          "x-api-key" myapikey
          authorization "Bearer token"
        }
        type "mcp"
      }
      {
        another_tool "value"
      }
    ]
    block_string #"
      hello there my friend
      raw strings preserve their contents
    "#
  }
}

//----
//- diagnostics
// E0010
//
//   × `client<llm>` config blocks are removed; declare a client value instead: `client MyClient = openai.OpenAiClient.new(model = "...");` (compose reliability with ai.Retry / ai.Fallback /
//   │ ai.RoundRobin)
//     ╭─[dictionary_valid_dictionary.baml:1:1]
//   1 │ ╭─▶ client<llm> MyClient {
//   2 │ │     provider openai
//   3 │ │     // dictionary test
//   4 │ │     options {
//   5 │ │       // no commas
//   6 │ │       max_tokens 100
//   7 │ │       array_key_with_quoted_unquoted [ "abc", "abc", "ccc" ]
//   8 │ │       // separate by commas
//   9 │ │       key2 "some random value", hello "world", thing "hello"
//  10 │ │        block_string #"hello there my frien
//  11 │ │                   d"#
//  12 │ │       inline_raw_string #"inline "raw" string,,"#
//  13 │ │       "string key" "value with spaces"
//  14 │ │       booleanValue true
//  15 │ │       stringKey "true"
//  16 │ │       array1 [
//  17 │ │         "one"
//  18 │ │         "two"
//  19 │ │       ]
//  20 │ │       // a comment and new empty_lines
//  21 │ │
//  22 │ │       array2 [ "one", "two" ]
//  23 │ │       array3Numbers [ 1, 2, 3 ]
//  24 │ │       nestedKey {
//  25 │ │         key "value"
//  26 │ │         key2 "value2"
//  27 │ │         nestedArray [ "yes", "queen"]
//  28 │ │         number 10
//  29 │ │       }
//  30 │ │       oneMoreValueToTestBrokenness "value"
//  31 │ │       myInteger 100
//  32 │ │       // array of objects with nested config blocks (no colons)
//  33 │ │       tools [
//  34 │ │         {
//  35 │ │           server_label "parallel_web_search"
//  36 │ │           server_url "https://example.com"
//  37 │ │           headers {
//  38 │ │             "x-api-key" myapikey
//  39 │ │             authorization "Bearer token"
//  40 │ │           }
//  41 │ │           type "mcp"
//  42 │ │         }
//  43 │ │         {
//  44 │ │           another_tool "value"
//  45 │ │         }
//  46 │ │       ]
//  47 │ │       block_string #"
//  48 │ │         hello there my friend
//  49 │ │         raw strings preserve their contents
//  50 │ │       "#
//  51 │ │     }
//  52 │ ├─▶ }
//     · ╰──── replace with `client Name = <expr>;`
//     ╰────
