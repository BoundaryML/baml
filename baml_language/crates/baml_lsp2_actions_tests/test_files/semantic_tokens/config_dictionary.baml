// The colon-free config dictionary surface now lives in test args blocks.
function DictionaryProbe(payload: string) -> string {
    client: "openai/gpt-4o-mini"
    prompt: `Use ${payload} ${ctx.output_format}`
}

test DictionaryTest {
    functions: [DictionaryProbe],
    // dictionary test
    args: {
        payload: "hello",
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
    },
}

//----
//- diagnostics
// <no-diagnostics-expected>
//
//- semantic_tokens
// config_dictionary.baml:1:1 (comment) len=74 "// The colon-free config dictionary surface now lives in test args blocks."
// config_dictionary.baml:2:1 (keyword) len=8 "function"
// config_dictionary.baml:2:10 (function) [declaration] len=15 "DictionaryProbe"
// config_dictionary.baml:2:26 (parameter) [declaration] len=7 "payload"
// config_dictionary.baml:2:35 (type) [defaultLibrary] len=6 "string"
// config_dictionary.baml:2:46 (type) [defaultLibrary] len=6 "string"
// config_dictionary.baml:3:5 (property) len=6 "client"
// config_dictionary.baml:3:13 (string) len=20 "\"openai/gpt-4o-mini\""
// config_dictionary.baml:4:5 (property) [declaration] len=6 "prompt"
// config_dictionary.baml:4:13 (string) len=1 "`"
// config_dictionary.baml:4:14 (string) len=3 "Use"
// config_dictionary.baml:4:20 (parameter) len=7 "payload"
// config_dictionary.baml:4:35 (property) len=13 "output_format"
// config_dictionary.baml:4:49 (string) len=1 "`"
// config_dictionary.baml:7:1 (keyword) len=4 "test"
// config_dictionary.baml:7:6 (struct) [declaration] len=14 "DictionaryTest"
// config_dictionary.baml:8:5 (property) len=9 "functions"
// config_dictionary.baml:9:5 (comment) len=18 "// dictionary test"
// config_dictionary.baml:10:5 (property) len=4 "args"
// config_dictionary.baml:11:9 (property) len=7 "payload"
// config_dictionary.baml:11:18 (string) len=7 "\"hello\""
// config_dictionary.baml:12:9 (comment) len=12 "// no commas"
// config_dictionary.baml:13:9 (property) len=10 "max_tokens"
// config_dictionary.baml:13:21 (number) len=3 "100"
// config_dictionary.baml:14:9 (property) len=30 "array_key_with_quoted_unquoted"
// config_dictionary.baml:14:42 (string) len=5 "\"abc\""
// config_dictionary.baml:14:49 (string) len=5 "\"abc\""
// config_dictionary.baml:14:56 (string) len=5 "\"ccc\""
// config_dictionary.baml:15:9 (comment) len=21 "// separate by commas"
// config_dictionary.baml:16:9 (property) len=4 "key2"
// config_dictionary.baml:16:15 (string) len=19 "\"some random value\""
// config_dictionary.baml:17:9 (property) len=5 "hello"
// config_dictionary.baml:17:16 (string) len=7 "\"world\""
// config_dictionary.baml:18:9 (property) len=5 "thing"
// config_dictionary.baml:18:16 (string) len=7 "\"hello\""
// config_dictionary.baml:19:9 (property) len=12 "block_string"
// config_dictionary.baml:19:23 (string) len=1 "`"
// config_dictionary.baml:20:13 (string) len=5 "hello"
// config_dictionary.baml:20:19 (string) len=5 "there"
// config_dictionary.baml:20:25 (string) len=2 "my"
// config_dictionary.baml:20:28 (string) len=5 "frien"
// config_dictionary.baml:21:29 (string) len=1 "d"
// config_dictionary.baml:22:9 (string) len=1 "`"
// config_dictionary.baml:23:9 (property) len=17 "inline_raw_string"
// config_dictionary.baml:23:28 (string) len=1 "`"
// config_dictionary.baml:23:29 (string) len=6 "inline"
// config_dictionary.baml:23:36 (string) len=1 "\""
// config_dictionary.baml:23:37 (string) len=3 "raw"
// config_dictionary.baml:23:40 (string) len=1 "\""
// config_dictionary.baml:23:42 (string) len=6 "string"
// config_dictionary.baml:23:48 (string) len=1 ","
// config_dictionary.baml:23:49 (string) len=1 ","
// config_dictionary.baml:23:50 (string) len=1 "`"
// config_dictionary.baml:24:9 (string) len=12 "\"string key\""
// config_dictionary.baml:24:23 (string) len=19 "\"value with spaces\""
// config_dictionary.baml:25:9 (property) len=12 "booleanValue"
// config_dictionary.baml:25:23 (boolean) len=4 "true"
// config_dictionary.baml:26:9 (property) len=9 "stringKey"
// config_dictionary.baml:26:20 (string) len=6 "\"true\""
// config_dictionary.baml:27:9 (property) len=6 "array1"
// config_dictionary.baml:27:18 (string) len=5 "\"one\""
// config_dictionary.baml:27:25 (string) len=5 "\"two\""
// config_dictionary.baml:28:9 (comment) len=32 "// a comment and new empty_lines"
// config_dictionary.baml:30:9 (property) len=6 "array2"
// config_dictionary.baml:30:18 (string) len=5 "\"one\""
// config_dictionary.baml:30:25 (string) len=5 "\"two\""
// config_dictionary.baml:31:9 (property) len=13 "array3Numbers"
// config_dictionary.baml:31:25 (number) len=1 "1"
// config_dictionary.baml:31:28 (number) len=1 "2"
// config_dictionary.baml:31:31 (number) len=1 "3"
// config_dictionary.baml:32:9 (property) len=9 "nestedKey"
// config_dictionary.baml:33:13 (property) len=3 "key"
// config_dictionary.baml:33:18 (string) len=7 "\"value\""
// config_dictionary.baml:34:13 (property) len=4 "key2"
// config_dictionary.baml:34:19 (string) len=8 "\"value2\""
// config_dictionary.baml:35:13 (property) len=11 "nestedArray"
// config_dictionary.baml:35:27 (string) len=5 "\"yes\""
// config_dictionary.baml:35:34 (string) len=7 "\"queen\""
// config_dictionary.baml:36:13 (property) len=6 "number"
// config_dictionary.baml:36:21 (number) len=2 "10"
// config_dictionary.baml:38:9 (property) len=28 "oneMoreValueToTestBrokenness"
// config_dictionary.baml:38:39 (string) len=7 "\"value\""
// config_dictionary.baml:39:9 (property) len=9 "myInteger"
// config_dictionary.baml:39:20 (number) len=3 "100"
// config_dictionary.baml:40:9 (comment) len=57 "// array of objects with nested config blocks (no colons)"
// config_dictionary.baml:41:9 (property) len=5 "tools"
// config_dictionary.baml:43:17 (property) len=12 "server_label"
// config_dictionary.baml:43:31 (string) len=21 "\"parallel_web_search\""
// config_dictionary.baml:44:17 (property) len=10 "server_url"
// config_dictionary.baml:44:29 (string) len=21 "\"https://example.com\""
// config_dictionary.baml:45:17 (property) len=7 "headers"
// config_dictionary.baml:46:21 (string) len=11 "\"x-api-key\""
// config_dictionary.baml:47:21 (property) len=13 "authorization"
// config_dictionary.baml:47:36 (string) len=14 "\"Bearer token\""
// config_dictionary.baml:49:17 (property) len=4 "type"
// config_dictionary.baml:49:23 (string) len=5 "\"mcp\""
// config_dictionary.baml:52:17 (property) len=12 "another_tool"
// config_dictionary.baml:52:31 (string) len=7 "\"value\""
