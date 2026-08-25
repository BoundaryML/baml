// Namespace root.sentiment — a tiny sentiment classifier whose real purpose
// is to teach TESTING in BAML. This file is just the function under test;
// the eval machinery (judge, synthetic data, file/API case loading) lives in
// eval.baml, and the guided tour of testsets in tests.baml.

// Cheap + fast model for classification.
client Mini = anthropic.AnthropicClient.new(
    model = "claude-haiku-4-5",
    api_key = env.ANTHROPIC_API_KEY,
    max_tokens = 1024,
);

type Label = "positive" | "negative" | "neutral";

class Verdict {
    label: Label,
    confidence: float,
}

// The function under test.
function classify(text: string) -> Verdict {
    client: Mini
    prompt: `
        Classify the sentiment of the text below as positive, negative, or
        neutral. Sarcasm counts as the sentiment actually expressed, not the
        literal words. Plain statements of fact are neutral.

        Text: ${text}

        ${ctx.output_format()}
    `
}

//----
//- diagnostics
// <no-diagnostics-expected>
//
//- semantic_tokens
// ns_sentiment.baml:1:1 (comment) len=78 "// Namespace root.sentiment — a tiny sentiment classifier whose real purpose"
// ns_sentiment.baml:2:1 (comment) len=74 "// is to teach TESTING in BAML. This file is just the function under test;"
// ns_sentiment.baml:3:1 (comment) len=77 "// the eval machinery (judge, synthetic data, file/API case loading) lives in"
// ns_sentiment.baml:4:1 (comment) len=60 "// eval.baml, and the guided tour of testsets in tests.baml."
// ns_sentiment.baml:6:1 (comment) len=41 "// Cheap + fast model for classification."
// ns_sentiment.baml:7:1 (keyword) len=6 "client"
// ns_sentiment.baml:7:13 (operator) len=1 "="
// ns_sentiment.baml:7:15 (namespace) [defaultLibrary] len=9 "anthropic"
// ns_sentiment.baml:7:24 (namespace) [defaultLibrary] len=1 "."
// ns_sentiment.baml:7:25 (class) [defaultLibrary] len=15 "AnthropicClient"
// ns_sentiment.baml:7:41 (method) len=3 "new"
// ns_sentiment.baml:8:5 (parameter) len=5 "model"
// ns_sentiment.baml:8:11 (operator) len=1 "="
// ns_sentiment.baml:8:13 (string) len=18 "\"claude-haiku-4-5\""
// ns_sentiment.baml:9:5 (parameter) len=7 "api_key"
// ns_sentiment.baml:9:13 (operator) len=1 "="
// ns_sentiment.baml:10:5 (parameter) len=10 "max_tokens"
// ns_sentiment.baml:10:16 (operator) len=1 "="
// ns_sentiment.baml:10:18 (number) len=4 "1024"
// ns_sentiment.baml:13:1 (keyword) len=4 "type"
// ns_sentiment.baml:13:6 (type) [declaration] len=5 "Label"
// ns_sentiment.baml:13:12 (operator) len=1 "="
// ns_sentiment.baml:13:14 (string) len=10 "\"positive\""
// ns_sentiment.baml:13:25 (operator) len=1 "|"
// ns_sentiment.baml:13:27 (string) len=10 "\"negative\""
// ns_sentiment.baml:13:38 (operator) len=1 "|"
// ns_sentiment.baml:13:40 (string) len=9 "\"neutral\""
// ns_sentiment.baml:15:1 (keyword) len=5 "class"
// ns_sentiment.baml:15:7 (class) [declaration] len=7 "Verdict"
// ns_sentiment.baml:16:5 (property) [declaration] len=5 "label"
// ns_sentiment.baml:16:12 (type) len=5 "Label"
// ns_sentiment.baml:17:5 (property) [declaration] len=10 "confidence"
// ns_sentiment.baml:17:17 (type) [defaultLibrary] len=5 "float"
// ns_sentiment.baml:20:1 (comment) len=27 "// The function under test."
// ns_sentiment.baml:21:1 (keyword) len=8 "function"
// ns_sentiment.baml:21:10 (function) [declaration] len=8 "classify"
// ns_sentiment.baml:21:19 (parameter) [declaration] len=4 "text"
// ns_sentiment.baml:21:25 (type) [defaultLibrary] len=6 "string"
// ns_sentiment.baml:21:36 (class) len=7 "Verdict"
// ns_sentiment.baml:22:5 (property) len=6 "client"
// ns_sentiment.baml:22:13 (variable) len=4 "Mini"
// ns_sentiment.baml:23:5 (property) [declaration] len=6 "prompt"
// ns_sentiment.baml:23:13 (string) len=1 "`"
// ns_sentiment.baml:24:9 (string) len=8 "Classify"
// ns_sentiment.baml:24:18 (string) len=3 "the"
// ns_sentiment.baml:24:22 (string) len=9 "sentiment"
// ns_sentiment.baml:24:32 (string) len=2 "of"
// ns_sentiment.baml:24:35 (string) len=3 "the"
// ns_sentiment.baml:24:39 (string) len=4 "text"
// ns_sentiment.baml:24:44 (string) len=5 "below"
// ns_sentiment.baml:24:50 (string) len=2 "as"
// ns_sentiment.baml:24:53 (string) len=8 "positive"
// ns_sentiment.baml:24:61 (string) len=1 ","
// ns_sentiment.baml:24:63 (string) len=8 "negative"
// ns_sentiment.baml:24:71 (string) len=1 ","
// ns_sentiment.baml:24:73 (string) len=2 "or"
// ns_sentiment.baml:25:9 (string) len=7 "neutral"
// ns_sentiment.baml:25:16 (string) len=1 "."
// ns_sentiment.baml:25:18 (string) len=7 "Sarcasm"
// ns_sentiment.baml:25:26 (string) len=6 "counts"
// ns_sentiment.baml:25:33 (string) len=2 "as"
// ns_sentiment.baml:25:36 (string) len=3 "the"
// ns_sentiment.baml:25:40 (string) len=9 "sentiment"
// ns_sentiment.baml:25:50 (string) len=8 "actually"
// ns_sentiment.baml:25:59 (string) len=9 "expressed"
// ns_sentiment.baml:25:68 (string) len=1 ","
// ns_sentiment.baml:25:70 (string) len=3 "not"
// ns_sentiment.baml:25:74 (string) len=3 "the"
// ns_sentiment.baml:26:9 (string) len=7 "literal"
// ns_sentiment.baml:26:17 (string) len=5 "words"
// ns_sentiment.baml:26:22 (string) len=1 "."
// ns_sentiment.baml:26:24 (string) len=5 "Plain"
// ns_sentiment.baml:26:30 (string) len=10 "statements"
// ns_sentiment.baml:26:41 (string) len=2 "of"
// ns_sentiment.baml:26:44 (string) len=4 "fact"
// ns_sentiment.baml:26:49 (string) len=3 "are"
// ns_sentiment.baml:26:53 (string) len=7 "neutral"
// ns_sentiment.baml:26:60 (string) len=1 "."
// ns_sentiment.baml:28:9 (string) len=4 "Text"
// ns_sentiment.baml:28:13 (string) len=1 ":"
// ns_sentiment.baml:28:17 (parameter) len=4 "text"
// ns_sentiment.baml:30:15 (method) len=13 "output_format"
// ns_sentiment.baml:31:5 (string) len=1 "`"
