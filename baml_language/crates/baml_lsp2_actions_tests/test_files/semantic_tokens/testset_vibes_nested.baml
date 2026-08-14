client GPT4o = openai.OpenAiClient.new(
    model = "gpt-4o",
    api_key = env.OPENAI_API_KEY,
);

class Sentiment {
    feeling string
    confidence float
    reasoning string
}

function ClassifySentiment(text: string) -> Sentiment {
    client: GPT4o
    prompt: `classify ${text}`
}

function GenerateTests(count: int, topic: string) -> string[] {
    client: GPT4o
    prompt: `generate ${count} tests about ${topic}`
}

testset "test" {
    let topics = ["happy", "sad"];
    for (let sentiments in topics) {
        testset sentiments {
            let req = baml.http.fetch("http://localhost:8000/" + sentiments);
            let data = req.text();
            let tests = GenerateTests$parse(data);
            for (let ex in tests) {
                test ex {
                    let result = ClassifySentiment("hi");
                    assert.equal(result.feeling, "positive");
                }
            }
        }
    }

    testset "vibes" {
        let topics = ["happy", "sad"];
        for (let sentiments in topics) {
            testset sentiments {
                let tests = GenerateTests(5, sentiments);
                for (let ex in tests) {
                    test ex {
                        let result = ClassifySentiment("hi");
                        assert.equal(result.feeling, "positive");
                    }
                }
            }
        }
    }
}

function Foo() -> string | image {
    let x = "foo";
    let y = 2;
    for (let i = 0; i < y; i += 1) {
        x = "hi:  " + x
    }
    x
}

function ClassifySentiment2(text: string) -> string {
    client: GPT4o
    prompt: `classify ${text}`
}

//----
//- diagnostics
// <no-diagnostics-expected>
//
//- semantic_tokens
// testset_vibes_nested.baml:1:1 (keyword) len=6 "client"
// testset_vibes_nested.baml:1:14 (operator) len=1 "="
// testset_vibes_nested.baml:1:16 (namespace) [defaultLibrary] len=6 "openai"
// testset_vibes_nested.baml:1:22 (namespace) [defaultLibrary] len=1 "."
// testset_vibes_nested.baml:1:23 (class) [defaultLibrary] len=12 "OpenAiClient"
// testset_vibes_nested.baml:1:36 (method) len=3 "new"
// testset_vibes_nested.baml:2:5 (parameter) len=5 "model"
// testset_vibes_nested.baml:2:11 (operator) len=1 "="
// testset_vibes_nested.baml:2:13 (string) len=8 "\"gpt-4o\""
// testset_vibes_nested.baml:3:5 (parameter) len=7 "api_key"
// testset_vibes_nested.baml:3:13 (operator) len=1 "="
// testset_vibes_nested.baml:6:1 (keyword) len=5 "class"
// testset_vibes_nested.baml:6:7 (class) [declaration] len=9 "Sentiment"
// testset_vibes_nested.baml:7:5 (property) [declaration] len=7 "feeling"
// testset_vibes_nested.baml:7:13 (type) [defaultLibrary] len=6 "string"
// testset_vibes_nested.baml:8:5 (property) [declaration] len=10 "confidence"
// testset_vibes_nested.baml:8:16 (type) [defaultLibrary] len=5 "float"
// testset_vibes_nested.baml:9:5 (property) [declaration] len=9 "reasoning"
// testset_vibes_nested.baml:9:15 (type) [defaultLibrary] len=6 "string"
// testset_vibes_nested.baml:12:1 (keyword) len=8 "function"
// testset_vibes_nested.baml:12:10 (function) [declaration] len=17 "ClassifySentiment"
// testset_vibes_nested.baml:12:28 (parameter) [declaration] len=4 "text"
// testset_vibes_nested.baml:12:34 (type) [defaultLibrary] len=6 "string"
// testset_vibes_nested.baml:12:45 (class) len=9 "Sentiment"
// testset_vibes_nested.baml:13:5 (property) len=6 "client"
// testset_vibes_nested.baml:13:13 (variable) len=5 "GPT4o"
// testset_vibes_nested.baml:14:5 (property) [declaration] len=6 "prompt"
// testset_vibes_nested.baml:14:13 (string) len=1 "`"
// testset_vibes_nested.baml:14:14 (string) len=8 "classify"
// testset_vibes_nested.baml:14:25 (parameter) len=4 "text"
// testset_vibes_nested.baml:14:30 (string) len=1 "`"
// testset_vibes_nested.baml:17:1 (keyword) len=8 "function"
// testset_vibes_nested.baml:17:10 (function) [declaration] len=13 "GenerateTests"
// testset_vibes_nested.baml:17:24 (parameter) [declaration] len=5 "count"
// testset_vibes_nested.baml:17:31 (type) [defaultLibrary] len=3 "int"
// testset_vibes_nested.baml:17:36 (parameter) [declaration] len=5 "topic"
// testset_vibes_nested.baml:17:43 (type) [defaultLibrary] len=6 "string"
// testset_vibes_nested.baml:17:54 (type) [defaultLibrary] len=6 "string"
// testset_vibes_nested.baml:18:5 (property) len=6 "client"
// testset_vibes_nested.baml:18:13 (variable) len=5 "GPT4o"
// testset_vibes_nested.baml:19:5 (property) [declaration] len=6 "prompt"
// testset_vibes_nested.baml:19:13 (string) len=1 "`"
// testset_vibes_nested.baml:19:14 (string) len=8 "generate"
// testset_vibes_nested.baml:19:25 (parameter) len=5 "count"
// testset_vibes_nested.baml:19:32 (string) len=5 "tests"
// testset_vibes_nested.baml:19:38 (string) len=5 "about"
// testset_vibes_nested.baml:19:46 (parameter) len=5 "topic"
// testset_vibes_nested.baml:19:52 (string) len=1 "`"
// testset_vibes_nested.baml:22:1 (keyword) len=7 "testset"
// testset_vibes_nested.baml:22:9 (string) len=6 "\"test\""
// testset_vibes_nested.baml:23:5 (keyword) len=3 "let"
// testset_vibes_nested.baml:23:9 (variable) [declaration] len=6 "topics"
// testset_vibes_nested.baml:23:16 (operator) len=1 "="
// testset_vibes_nested.baml:23:19 (string) len=7 "\"happy\""
// testset_vibes_nested.baml:23:28 (string) len=5 "\"sad\""
// testset_vibes_nested.baml:24:5 (keyword) len=3 "for"
// testset_vibes_nested.baml:24:10 (keyword) len=3 "let"
// testset_vibes_nested.baml:24:14 (variable) [declaration] len=10 "sentiments"
// testset_vibes_nested.baml:24:25 (keyword) len=2 "in"
// testset_vibes_nested.baml:24:28 (variable) len=6 "topics"
// testset_vibes_nested.baml:25:9 (keyword) len=7 "testset"
// testset_vibes_nested.baml:25:17 (variable) len=10 "sentiments"
// testset_vibes_nested.baml:26:13 (keyword) len=3 "let"
// testset_vibes_nested.baml:26:17 (variable) [declaration] len=3 "req"
// testset_vibes_nested.baml:26:21 (operator) len=1 "="
// testset_vibes_nested.baml:26:23 (namespace) [defaultLibrary] len=4 "baml"
// testset_vibes_nested.baml:26:27 (namespace) [defaultLibrary] len=1 "."
// testset_vibes_nested.baml:26:28 (namespace) [defaultLibrary] len=4 "http"
// testset_vibes_nested.baml:26:32 (namespace) [defaultLibrary] len=1 "."
// testset_vibes_nested.baml:26:33 (function) len=5 "fetch"
// testset_vibes_nested.baml:26:39 (string) len=24 "\"http://localhost:8000/\""
// testset_vibes_nested.baml:26:64 (operator) len=1 "+"
// testset_vibes_nested.baml:26:66 (variable) len=10 "sentiments"
// testset_vibes_nested.baml:27:13 (keyword) len=3 "let"
// testset_vibes_nested.baml:27:17 (variable) [declaration] len=4 "data"
// testset_vibes_nested.baml:27:22 (operator) len=1 "="
// testset_vibes_nested.baml:27:24 (variable) len=3 "req"
// testset_vibes_nested.baml:27:28 (method) len=4 "text"
// testset_vibes_nested.baml:28:13 (keyword) len=3 "let"
// testset_vibes_nested.baml:28:17 (variable) [declaration] len=5 "tests"
// testset_vibes_nested.baml:28:23 (operator) len=1 "="
// testset_vibes_nested.baml:28:25 (function) len=19 "GenerateTests$parse"
// testset_vibes_nested.baml:28:45 (variable) len=4 "data"
// testset_vibes_nested.baml:29:13 (keyword) len=3 "for"
// testset_vibes_nested.baml:29:18 (keyword) len=3 "let"
// testset_vibes_nested.baml:29:22 (variable) [declaration] len=2 "ex"
// testset_vibes_nested.baml:29:25 (keyword) len=2 "in"
// testset_vibes_nested.baml:29:28 (variable) len=5 "tests"
// testset_vibes_nested.baml:30:17 (keyword) len=4 "test"
// testset_vibes_nested.baml:30:22 (variable) len=2 "ex"
// testset_vibes_nested.baml:31:21 (keyword) len=3 "let"
// testset_vibes_nested.baml:31:25 (variable) [declaration] len=6 "result"
// testset_vibes_nested.baml:31:32 (operator) len=1 "="
// testset_vibes_nested.baml:31:34 (function) len=17 "ClassifySentiment"
// testset_vibes_nested.baml:31:52 (string) len=4 "\"hi\""
// testset_vibes_nested.baml:32:21 (namespace) [defaultLibrary] len=6 "assert"
// testset_vibes_nested.baml:32:27 (namespace) [defaultLibrary] len=1 "."
// testset_vibes_nested.baml:32:28 (function) len=5 "equal"
// testset_vibes_nested.baml:32:34 (variable) len=6 "result"
// testset_vibes_nested.baml:32:41 (property) len=7 "feeling"
// testset_vibes_nested.baml:32:50 (string) len=10 "\"positive\""
// testset_vibes_nested.baml:38:5 (keyword) len=7 "testset"
// testset_vibes_nested.baml:38:13 (string) len=7 "\"vibes\""
// testset_vibes_nested.baml:39:9 (keyword) len=3 "let"
// testset_vibes_nested.baml:39:13 (variable) [declaration] len=6 "topics"
// testset_vibes_nested.baml:39:20 (operator) len=1 "="
// testset_vibes_nested.baml:39:23 (string) len=7 "\"happy\""
// testset_vibes_nested.baml:39:32 (string) len=5 "\"sad\""
// testset_vibes_nested.baml:40:9 (keyword) len=3 "for"
// testset_vibes_nested.baml:40:14 (keyword) len=3 "let"
// testset_vibes_nested.baml:40:18 (variable) [declaration] len=10 "sentiments"
// testset_vibes_nested.baml:40:29 (keyword) len=2 "in"
// testset_vibes_nested.baml:40:32 (variable) len=6 "topics"
// testset_vibes_nested.baml:41:13 (keyword) len=7 "testset"
// testset_vibes_nested.baml:41:21 (variable) len=10 "sentiments"
// testset_vibes_nested.baml:42:17 (keyword) len=3 "let"
// testset_vibes_nested.baml:42:21 (variable) [declaration] len=5 "tests"
// testset_vibes_nested.baml:42:27 (operator) len=1 "="
// testset_vibes_nested.baml:42:29 (function) len=13 "GenerateTests"
// testset_vibes_nested.baml:42:43 (number) len=1 "5"
// testset_vibes_nested.baml:42:46 (variable) len=10 "sentiments"
// testset_vibes_nested.baml:43:17 (keyword) len=3 "for"
// testset_vibes_nested.baml:43:22 (keyword) len=3 "let"
// testset_vibes_nested.baml:43:26 (variable) [declaration] len=2 "ex"
// testset_vibes_nested.baml:43:29 (keyword) len=2 "in"
// testset_vibes_nested.baml:43:32 (variable) len=5 "tests"
// testset_vibes_nested.baml:44:21 (keyword) len=4 "test"
// testset_vibes_nested.baml:44:26 (variable) len=2 "ex"
// testset_vibes_nested.baml:45:25 (keyword) len=3 "let"
// testset_vibes_nested.baml:45:29 (variable) [declaration] len=6 "result"
// testset_vibes_nested.baml:45:36 (operator) len=1 "="
// testset_vibes_nested.baml:45:38 (function) len=17 "ClassifySentiment"
// testset_vibes_nested.baml:45:56 (string) len=4 "\"hi\""
// testset_vibes_nested.baml:46:25 (namespace) [defaultLibrary] len=6 "assert"
// testset_vibes_nested.baml:46:31 (namespace) [defaultLibrary] len=1 "."
// testset_vibes_nested.baml:46:32 (function) len=5 "equal"
// testset_vibes_nested.baml:46:38 (variable) len=6 "result"
// testset_vibes_nested.baml:46:45 (property) len=7 "feeling"
// testset_vibes_nested.baml:46:54 (string) len=10 "\"positive\""
// testset_vibes_nested.baml:54:1 (keyword) len=8 "function"
// testset_vibes_nested.baml:54:10 (function) [declaration] len=3 "Foo"
// testset_vibes_nested.baml:54:19 (type) [defaultLibrary] len=6 "string"
// testset_vibes_nested.baml:54:26 (operator) len=1 "|"
// testset_vibes_nested.baml:54:28 (type) [defaultLibrary] len=5 "image"
// testset_vibes_nested.baml:55:5 (keyword) len=3 "let"
// testset_vibes_nested.baml:55:9 (variable) [declaration] len=1 "x"
// testset_vibes_nested.baml:55:11 (operator) len=1 "="
// testset_vibes_nested.baml:55:13 (string) len=5 "\"foo\""
// testset_vibes_nested.baml:56:5 (keyword) len=3 "let"
// testset_vibes_nested.baml:56:9 (variable) [declaration] len=1 "y"
// testset_vibes_nested.baml:56:11 (operator) len=1 "="
// testset_vibes_nested.baml:56:13 (number) len=1 "2"
// testset_vibes_nested.baml:57:5 (keyword) len=3 "for"
// testset_vibes_nested.baml:57:10 (keyword) len=3 "let"
// testset_vibes_nested.baml:57:14 (variable) [declaration] len=1 "i"
// testset_vibes_nested.baml:57:16 (operator) len=1 "="
// testset_vibes_nested.baml:57:18 (number) len=1 "0"
// testset_vibes_nested.baml:57:21 (variable) len=1 "i"
// testset_vibes_nested.baml:57:23 (operator) len=1 "<"
// testset_vibes_nested.baml:57:25 (variable) len=1 "y"
// testset_vibes_nested.baml:57:28 (variable) len=1 "i"
// testset_vibes_nested.baml:57:30 (operator) len=2 "+="
// testset_vibes_nested.baml:57:33 (number) len=1 "1"
// testset_vibes_nested.baml:58:9 (variable) len=1 "x"
// testset_vibes_nested.baml:58:11 (operator) len=1 "="
// testset_vibes_nested.baml:58:13 (string) len=7 "\"hi:  \""
// testset_vibes_nested.baml:58:21 (operator) len=1 "+"
// testset_vibes_nested.baml:58:23 (variable) len=1 "x"
// testset_vibes_nested.baml:60:5 (variable) len=1 "x"
// testset_vibes_nested.baml:63:1 (keyword) len=8 "function"
// testset_vibes_nested.baml:63:10 (function) [declaration] len=18 "ClassifySentiment2"
// testset_vibes_nested.baml:63:29 (parameter) [declaration] len=4 "text"
// testset_vibes_nested.baml:63:35 (type) [defaultLibrary] len=6 "string"
// testset_vibes_nested.baml:63:46 (type) [defaultLibrary] len=6 "string"
// testset_vibes_nested.baml:64:5 (property) len=6 "client"
// testset_vibes_nested.baml:64:13 (variable) len=5 "GPT4o"
// testset_vibes_nested.baml:65:5 (property) [declaration] len=6 "prompt"
// testset_vibes_nested.baml:65:13 (string) len=1 "`"
// testset_vibes_nested.baml:65:14 (string) len=8 "classify"
// testset_vibes_nested.baml:65:25 (parameter) len=4 "text"
// testset_vibes_nested.baml:65:30 (string) len=1 "`"
