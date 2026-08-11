client<llm> MyClient {
    provider: openai,
    // dictionary test
    options: {
        // no commas
        max_tokens: 100,
        array_key_with_quoted_unquoted: ["abc", "abc", "ccc"],
        // separate by commas
        key2: "some random value",
        hello: "world",
        thing: "hello",
        block_string: `
            hello there my frien
                            d
        `,
        inline_raw_string: `inline "raw" string,,`,
        "string key": "value with spaces",
        booleanValue: true,
        stringKey: "true",
        array1: ["one", "two"],
        // a comment and new empty_lines

        array2: ["one", "two"],
        array3Numbers: [1, 2, 3],
        nestedKey: {
            key: "value",
            key2: "value2",
            nestedArray: ["yes", "queen"],
            number: 10,
        },
        oneMoreValueToTestBrokenness: "value",
        myInteger: 100,
        // array of objects with nested config blocks (no colons)
        tools: [
            {
                server_label: "parallel_web_search",
                server_url: "https://example.com",
                headers: {
                    "x-api-key": myapikey,
                    authorization: "Bearer token",
                },
                type: "mcp",
            },
            {
                another_tool: "value",
            },
        ],
        block_string: `
            hello there my friend
        `,
    },
}

//----
//- diagnostics
// E0010
//
//   × `client<llm>` config blocks are removed; declare a client value instead: `client MyClient = openai.OpenAiClient.new(model = "...");` (compose reliability with ai.Retry / ai.Fallback /
//   │ ai.RoundRobin)
//     ╭─[dictionary_valid_dictionary.baml:1:1]
//   1 │ ╭─▶ client<llm> MyClient {
//   2 │ │       provider: openai,
//   3 │ │       // dictionary test
//   4 │ │       options: {
//   5 │ │           // no commas
//   6 │ │           max_tokens: 100,
//   7 │ │           array_key_with_quoted_unquoted: ["abc", "abc", "ccc"],
//   8 │ │           // separate by commas
//   9 │ │           key2: "some random value",
//  10 │ │           hello: "world",
//  11 │ │           thing: "hello",
//  12 │ │           block_string: `
//  13 │ │               hello there my frien
//  14 │ │                               d
//  15 │ │           `,
//  16 │ │           inline_raw_string: `inline "raw" string,,`,
//  17 │ │           "string key": "value with spaces",
//  18 │ │           booleanValue: true,
//  19 │ │           stringKey: "true",
//  20 │ │           array1: ["one", "two"],
//  21 │ │           // a comment and new empty_lines
//  22 │ │
//  23 │ │           array2: ["one", "two"],
//  24 │ │           array3Numbers: [1, 2, 3],
//  25 │ │           nestedKey: {
//  26 │ │               key: "value",
//  27 │ │               key2: "value2",
//  28 │ │               nestedArray: ["yes", "queen"],
//  29 │ │               number: 10,
//  30 │ │           },
//  31 │ │           oneMoreValueToTestBrokenness: "value",
//  32 │ │           myInteger: 100,
//  33 │ │           // array of objects with nested config blocks (no colons)
//  34 │ │           tools: [
//  35 │ │               {
//  36 │ │                   server_label: "parallel_web_search",
//  37 │ │                   server_url: "https://example.com",
//  38 │ │                   headers: {
//  39 │ │                       "x-api-key": myapikey,
//  40 │ │                       authorization: "Bearer token",
//  41 │ │                   },
//  42 │ │                   type: "mcp",
//  43 │ │               },
//  44 │ │               {
//  45 │ │                   another_tool: "value",
//  46 │ │               },
//  47 │ │           ],
//  48 │ │           block_string: `
//  49 │ │               hello there my friend
//  50 │ │           `,
//  51 │ │       },
//  52 │ ├─▶ }
//     · ╰──── replace with `client Name = <expr>;`
//     ╰────
