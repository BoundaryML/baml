client<llm> GPT4o {
    provider openai
    options {
        model "gpt-4o"
        api_key env.OPENAI_API_KEY
    }
}

class Sentiment {
    feeling string
    confidence float
    reasoning string
}

function ClassifySentiment(text: string) -> Sentiment {
    client GPT4o
    prompt #"classify {{ text }}"#
}

function GenerateTests(count: int, topic: string) -> string[] {
    client GPT4o
    prompt #"generate {{ count }} tests about {{ topic }}"#
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
    client GPT4o
    prompt #"classify {{ text }}"#
}

//----
//- diagnostics
// <no-diagnostics-expected>
//
//- semantic_tokens
// testset_vibes_nested.baml:1:1 (keyword) len=6 "client"
// testset_vibes_nested.baml:1:7 (operator) len=1 "<"
// testset_vibes_nested.baml:1:8 (type) len=3 "llm"
// testset_vibes_nested.baml:1:11 (operator) len=1 ">"
// testset_vibes_nested.baml:1:13 (struct) [declaration] len=5 "GPT4o"
// testset_vibes_nested.baml:2:5 (property) len=8 "provider"
// testset_vibes_nested.baml:3:5 (property) len=7 "options"
// testset_vibes_nested.baml:4:9 (property) len=5 "model"
// testset_vibes_nested.baml:4:15 (string) len=8 "\"gpt-4o\""
// testset_vibes_nested.baml:5:9 (property) len=7 "api_key"
// testset_vibes_nested.baml:9:1 (keyword) len=5 "class"
// testset_vibes_nested.baml:9:7 (class) [declaration] len=9 "Sentiment"
// testset_vibes_nested.baml:10:5 (property) [declaration] len=7 "feeling"
// testset_vibes_nested.baml:10:13 (type) [defaultLibrary] len=6 "string"
// testset_vibes_nested.baml:11:5 (property) [declaration] len=10 "confidence"
// testset_vibes_nested.baml:11:16 (type) [defaultLibrary] len=5 "float"
// testset_vibes_nested.baml:12:5 (property) [declaration] len=9 "reasoning"
// testset_vibes_nested.baml:12:15 (type) [defaultLibrary] len=6 "string"
// testset_vibes_nested.baml:15:1 (keyword) len=8 "function"
// testset_vibes_nested.baml:15:10 (function) [declaration] len=17 "ClassifySentiment"
// testset_vibes_nested.baml:15:28 (parameter) [declaration] len=4 "text"
// testset_vibes_nested.baml:15:34 (type) [defaultLibrary] len=6 "string"
// testset_vibes_nested.baml:15:45 (class) len=9 "Sentiment"
// testset_vibes_nested.baml:16:5 (property) len=6 "client"
// testset_vibes_nested.baml:17:5 (property) [declaration] len=6 "prompt"
// testset_vibes_nested.baml:17:12 (string) len=1 "#"
// testset_vibes_nested.baml:17:13 (string) len=1 "\""
// testset_vibes_nested.baml:17:14 (string) len=8 "classify"
// testset_vibes_nested.baml:17:23 (string) len=1 "{"
// testset_vibes_nested.baml:17:24 (string) len=1 "{"
// testset_vibes_nested.baml:17:26 (string) len=4 "text"
// testset_vibes_nested.baml:17:31 (string) len=1 "}"
// testset_vibes_nested.baml:17:32 (string) len=1 "}"
// testset_vibes_nested.baml:17:33 (string) len=1 "\""
// testset_vibes_nested.baml:17:34 (string) len=1 "#"
// testset_vibes_nested.baml:20:1 (keyword) len=8 "function"
// testset_vibes_nested.baml:20:10 (function) [declaration] len=13 "GenerateTests"
// testset_vibes_nested.baml:20:24 (parameter) [declaration] len=5 "count"
// testset_vibes_nested.baml:20:31 (type) [defaultLibrary] len=3 "int"
// testset_vibes_nested.baml:20:36 (parameter) [declaration] len=5 "topic"
// testset_vibes_nested.baml:20:43 (type) [defaultLibrary] len=6 "string"
// testset_vibes_nested.baml:20:54 (type) [defaultLibrary] len=6 "string"
// testset_vibes_nested.baml:21:5 (property) len=6 "client"
// testset_vibes_nested.baml:22:5 (property) [declaration] len=6 "prompt"
// testset_vibes_nested.baml:22:12 (string) len=1 "#"
// testset_vibes_nested.baml:22:13 (string) len=1 "\""
// testset_vibes_nested.baml:22:14 (string) len=8 "generate"
// testset_vibes_nested.baml:22:23 (string) len=1 "{"
// testset_vibes_nested.baml:22:24 (string) len=1 "{"
// testset_vibes_nested.baml:22:26 (string) len=5 "count"
// testset_vibes_nested.baml:22:32 (string) len=1 "}"
// testset_vibes_nested.baml:22:33 (string) len=1 "}"
// testset_vibes_nested.baml:22:35 (string) len=5 "tests"
// testset_vibes_nested.baml:22:41 (string) len=5 "about"
// testset_vibes_nested.baml:22:47 (string) len=1 "{"
// testset_vibes_nested.baml:22:48 (string) len=1 "{"
// testset_vibes_nested.baml:22:50 (string) len=5 "topic"
// testset_vibes_nested.baml:22:56 (string) len=1 "}"
// testset_vibes_nested.baml:22:57 (string) len=1 "}"
// testset_vibes_nested.baml:22:58 (string) len=1 "\""
// testset_vibes_nested.baml:22:59 (string) len=1 "#"
// testset_vibes_nested.baml:25:1 (keyword) len=7 "testset"
// testset_vibes_nested.baml:25:9 (string) len=6 "\"test\""
// testset_vibes_nested.baml:26:5 (keyword) len=3 "let"
// testset_vibes_nested.baml:26:9 (variable) [declaration] len=6 "topics"
// testset_vibes_nested.baml:26:16 (operator) len=1 "="
// testset_vibes_nested.baml:26:19 (string) len=7 "\"happy\""
// testset_vibes_nested.baml:26:28 (string) len=5 "\"sad\""
// testset_vibes_nested.baml:27:5 (keyword) len=3 "for"
// testset_vibes_nested.baml:27:10 (keyword) len=3 "let"
// testset_vibes_nested.baml:27:14 (variable) [declaration] len=10 "sentiments"
// testset_vibes_nested.baml:27:25 (keyword) len=2 "in"
// testset_vibes_nested.baml:27:28 (variable) len=6 "topics"
// testset_vibes_nested.baml:28:9 (keyword) len=7 "testset"
// testset_vibes_nested.baml:28:17 (variable) len=10 "sentiments"
// testset_vibes_nested.baml:29:13 (keyword) len=3 "let"
// testset_vibes_nested.baml:29:17 (variable) [declaration] len=3 "req"
// testset_vibes_nested.baml:29:21 (operator) len=1 "="
// testset_vibes_nested.baml:29:23 (namespace) len=4 "baml"
// testset_vibes_nested.baml:29:28 (namespace) len=4 "http"
// testset_vibes_nested.baml:29:33 (function) len=5 "fetch"
// testset_vibes_nested.baml:29:39 (string) len=24 "\"http://localhost:8000/\""
// testset_vibes_nested.baml:29:64 (operator) len=1 "+"
// testset_vibes_nested.baml:29:66 (variable) len=10 "sentiments"
// testset_vibes_nested.baml:30:13 (keyword) len=3 "let"
// testset_vibes_nested.baml:30:17 (variable) [declaration] len=4 "data"
// testset_vibes_nested.baml:30:22 (operator) len=1 "="
// testset_vibes_nested.baml:30:24 (variable) len=3 "req"
// testset_vibes_nested.baml:30:28 (method) len=4 "text"
// testset_vibes_nested.baml:31:13 (keyword) len=3 "let"
// testset_vibes_nested.baml:31:17 (variable) [declaration] len=5 "tests"
// testset_vibes_nested.baml:31:23 (operator) len=1 "="
// testset_vibes_nested.baml:31:25 (function) len=19 "GenerateTests$parse"
// testset_vibes_nested.baml:31:45 (variable) len=4 "data"
// testset_vibes_nested.baml:32:13 (keyword) len=3 "for"
// testset_vibes_nested.baml:32:18 (keyword) len=3 "let"
// testset_vibes_nested.baml:32:22 (variable) [declaration] len=2 "ex"
// testset_vibes_nested.baml:32:25 (keyword) len=2 "in"
// testset_vibes_nested.baml:32:28 (variable) len=5 "tests"
// testset_vibes_nested.baml:33:17 (keyword) len=4 "test"
// testset_vibes_nested.baml:33:22 (variable) len=2 "ex"
// testset_vibes_nested.baml:34:21 (keyword) len=3 "let"
// testset_vibes_nested.baml:34:25 (variable) [declaration] len=6 "result"
// testset_vibes_nested.baml:34:32 (operator) len=1 "="
// testset_vibes_nested.baml:34:34 (function) len=17 "ClassifySentiment"
// testset_vibes_nested.baml:34:52 (string) len=4 "\"hi\""
// testset_vibes_nested.baml:35:21 (namespace) len=6 "assert"
// testset_vibes_nested.baml:35:28 (function) len=5 "equal"
// testset_vibes_nested.baml:35:34 (variable) len=6 "result"
// testset_vibes_nested.baml:35:41 (property) len=7 "feeling"
// testset_vibes_nested.baml:35:50 (string) len=10 "\"positive\""
// testset_vibes_nested.baml:41:5 (keyword) len=7 "testset"
// testset_vibes_nested.baml:41:13 (string) len=7 "\"vibes\""
// testset_vibes_nested.baml:42:9 (keyword) len=3 "let"
// testset_vibes_nested.baml:42:13 (variable) [declaration] len=6 "topics"
// testset_vibes_nested.baml:42:20 (operator) len=1 "="
// testset_vibes_nested.baml:42:23 (string) len=7 "\"happy\""
// testset_vibes_nested.baml:42:32 (string) len=5 "\"sad\""
// testset_vibes_nested.baml:43:9 (keyword) len=3 "for"
// testset_vibes_nested.baml:43:14 (keyword) len=3 "let"
// testset_vibes_nested.baml:43:18 (variable) [declaration] len=10 "sentiments"
// testset_vibes_nested.baml:43:29 (keyword) len=2 "in"
// testset_vibes_nested.baml:43:32 (variable) len=6 "topics"
// testset_vibes_nested.baml:44:13 (keyword) len=7 "testset"
// testset_vibes_nested.baml:44:21 (variable) len=10 "sentiments"
// testset_vibes_nested.baml:45:17 (keyword) len=3 "let"
// testset_vibes_nested.baml:45:21 (variable) [declaration] len=5 "tests"
// testset_vibes_nested.baml:45:27 (operator) len=1 "="
// testset_vibes_nested.baml:45:29 (function) len=13 "GenerateTests"
// testset_vibes_nested.baml:45:43 (number) len=1 "5"
// testset_vibes_nested.baml:45:46 (variable) len=10 "sentiments"
// testset_vibes_nested.baml:46:17 (keyword) len=3 "for"
// testset_vibes_nested.baml:46:22 (keyword) len=3 "let"
// testset_vibes_nested.baml:46:26 (variable) [declaration] len=2 "ex"
// testset_vibes_nested.baml:46:29 (keyword) len=2 "in"
// testset_vibes_nested.baml:46:32 (variable) len=5 "tests"
// testset_vibes_nested.baml:47:21 (keyword) len=4 "test"
// testset_vibes_nested.baml:47:26 (variable) len=2 "ex"
// testset_vibes_nested.baml:48:25 (keyword) len=3 "let"
// testset_vibes_nested.baml:48:29 (variable) [declaration] len=6 "result"
// testset_vibes_nested.baml:48:36 (operator) len=1 "="
// testset_vibes_nested.baml:48:38 (function) len=17 "ClassifySentiment"
// testset_vibes_nested.baml:48:56 (string) len=4 "\"hi\""
// testset_vibes_nested.baml:49:25 (namespace) len=6 "assert"
// testset_vibes_nested.baml:49:32 (function) len=5 "equal"
// testset_vibes_nested.baml:49:38 (variable) len=6 "result"
// testset_vibes_nested.baml:49:45 (property) len=7 "feeling"
// testset_vibes_nested.baml:49:54 (string) len=10 "\"positive\""
// testset_vibes_nested.baml:57:1 (keyword) len=8 "function"
// testset_vibes_nested.baml:57:10 (function) [declaration] len=3 "Foo"
// testset_vibes_nested.baml:57:19 (type) [defaultLibrary] len=6 "string"
// testset_vibes_nested.baml:57:26 (operator) len=1 "|"
// testset_vibes_nested.baml:57:28 (type) [defaultLibrary] len=5 "image"
// testset_vibes_nested.baml:58:5 (keyword) len=3 "let"
// testset_vibes_nested.baml:58:9 (variable) [declaration] len=1 "x"
// testset_vibes_nested.baml:58:11 (operator) len=1 "="
// testset_vibes_nested.baml:58:13 (string) len=5 "\"foo\""
// testset_vibes_nested.baml:59:5 (keyword) len=3 "let"
// testset_vibes_nested.baml:59:9 (variable) [declaration] len=1 "y"
// testset_vibes_nested.baml:59:11 (operator) len=1 "="
// testset_vibes_nested.baml:59:13 (number) len=1 "2"
// testset_vibes_nested.baml:60:5 (keyword) len=3 "for"
// testset_vibes_nested.baml:60:10 (keyword) len=3 "let"
// testset_vibes_nested.baml:60:14 (variable) [declaration] len=1 "i"
// testset_vibes_nested.baml:60:16 (operator) len=1 "="
// testset_vibes_nested.baml:60:18 (number) len=1 "0"
// testset_vibes_nested.baml:60:21 (variable) len=1 "i"
// testset_vibes_nested.baml:60:23 (operator) len=1 "<"
// testset_vibes_nested.baml:60:25 (variable) len=1 "y"
// testset_vibes_nested.baml:60:28 (variable) len=1 "i"
// testset_vibes_nested.baml:60:30 (operator) len=2 "+="
// testset_vibes_nested.baml:60:33 (number) len=1 "1"
// testset_vibes_nested.baml:61:9 (variable) len=1 "x"
// testset_vibes_nested.baml:61:11 (operator) len=1 "="
// testset_vibes_nested.baml:61:13 (string) len=7 "\"hi:  \""
// testset_vibes_nested.baml:61:21 (operator) len=1 "+"
// testset_vibes_nested.baml:61:23 (variable) len=1 "x"
// testset_vibes_nested.baml:63:5 (variable) len=1 "x"
// testset_vibes_nested.baml:66:1 (keyword) len=8 "function"
// testset_vibes_nested.baml:66:10 (function) [declaration] len=18 "ClassifySentiment2"
// testset_vibes_nested.baml:66:29 (parameter) [declaration] len=4 "text"
// testset_vibes_nested.baml:66:35 (type) [defaultLibrary] len=6 "string"
// testset_vibes_nested.baml:66:46 (type) [defaultLibrary] len=6 "string"
// testset_vibes_nested.baml:67:5 (property) len=6 "client"
// testset_vibes_nested.baml:68:5 (property) [declaration] len=6 "prompt"
// testset_vibes_nested.baml:68:12 (string) len=1 "#"
// testset_vibes_nested.baml:68:13 (string) len=1 "\""
// testset_vibes_nested.baml:68:14 (string) len=8 "classify"
// testset_vibes_nested.baml:68:23 (string) len=1 "{"
// testset_vibes_nested.baml:68:24 (string) len=1 "{"
// testset_vibes_nested.baml:68:26 (string) len=4 "text"
// testset_vibes_nested.baml:68:31 (string) len=1 "}"
// testset_vibes_nested.baml:68:32 (string) len=1 "}"
// testset_vibes_nested.baml:68:33 (string) len=1 "\""
// testset_vibes_nested.baml:68:34 (string) len=1 "#"
