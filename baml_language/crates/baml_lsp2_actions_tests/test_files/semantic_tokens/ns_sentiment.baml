// Namespace root.sentiment — a tiny sentiment classifier whose real purpose
// is to teach TESTING in BAML. This file is just the function under test;
// the eval machinery (judge, synthetic data, file/API case loading) lives in
// eval.baml, and the guided tour of testsets in tests.baml.

// Cheap + fast model for classification.
client<llm> Mini {
    provider: anthropic,
    options: {
        model: "claude-haiku-4-5",
        api_key: env.ANTHROPIC_API_KEY,
        max_tokens: 1024,
    },
}

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

        ${ctx.output_format}
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
// ns_sentiment.baml:7:7 (operator) len=1 "<"
// ns_sentiment.baml:7:8 (type) len=3 "llm"
// ns_sentiment.baml:7:11 (operator) len=1 ">"
// ns_sentiment.baml:7:13 (struct) [declaration] len=4 "Mini"
// ns_sentiment.baml:8:5 (property) len=8 "provider"
// ns_sentiment.baml:9:5 (property) len=7 "options"
// ns_sentiment.baml:10:9 (property) len=5 "model"
// ns_sentiment.baml:10:16 (string) len=18 "\"claude-haiku-4-5\""
// ns_sentiment.baml:11:9 (property) len=7 "api_key"
// ns_sentiment.baml:12:9 (property) len=10 "max_tokens"
// ns_sentiment.baml:12:21 (number) len=4 "1024"
// ns_sentiment.baml:16:1 (keyword) len=4 "type"
// ns_sentiment.baml:16:6 (type) [declaration] len=5 "Label"
// ns_sentiment.baml:16:12 (operator) len=1 "="
// ns_sentiment.baml:16:14 (string) len=10 "\"positive\""
// ns_sentiment.baml:16:25 (operator) len=1 "|"
// ns_sentiment.baml:16:27 (string) len=10 "\"negative\""
// ns_sentiment.baml:16:38 (operator) len=1 "|"
// ns_sentiment.baml:16:40 (string) len=9 "\"neutral\""
// ns_sentiment.baml:18:1 (keyword) len=5 "class"
// ns_sentiment.baml:18:7 (class) [declaration] len=7 "Verdict"
// ns_sentiment.baml:19:5 (property) [declaration] len=5 "label"
// ns_sentiment.baml:19:12 (type) len=5 "Label"
// ns_sentiment.baml:20:5 (property) [declaration] len=10 "confidence"
// ns_sentiment.baml:20:17 (type) [defaultLibrary] len=5 "float"
// ns_sentiment.baml:23:1 (comment) len=27 "// The function under test."
// ns_sentiment.baml:24:1 (keyword) len=8 "function"
// ns_sentiment.baml:24:10 (function) [declaration] len=8 "classify"
// ns_sentiment.baml:24:19 (parameter) [declaration] len=4 "text"
// ns_sentiment.baml:24:25 (type) [defaultLibrary] len=6 "string"
// ns_sentiment.baml:24:36 (class) len=7 "Verdict"
// ns_sentiment.baml:25:5 (property) len=6 "client"
// ns_sentiment.baml:26:5 (property) [declaration] len=6 "prompt"
// ns_sentiment.baml:26:13 (string) len=1 "`"
// ns_sentiment.baml:27:9 (string) len=8 "Classify"
// ns_sentiment.baml:27:18 (string) len=3 "the"
// ns_sentiment.baml:27:22 (string) len=9 "sentiment"
// ns_sentiment.baml:27:32 (string) len=2 "of"
// ns_sentiment.baml:27:35 (string) len=3 "the"
// ns_sentiment.baml:27:39 (string) len=4 "text"
// ns_sentiment.baml:27:44 (string) len=5 "below"
// ns_sentiment.baml:27:50 (string) len=2 "as"
// ns_sentiment.baml:27:53 (string) len=8 "positive"
// ns_sentiment.baml:27:61 (string) len=1 ","
// ns_sentiment.baml:27:63 (string) len=8 "negative"
// ns_sentiment.baml:27:71 (string) len=1 ","
// ns_sentiment.baml:27:73 (string) len=2 "or"
// ns_sentiment.baml:28:9 (string) len=7 "neutral"
// ns_sentiment.baml:28:16 (string) len=1 "."
// ns_sentiment.baml:28:18 (string) len=7 "Sarcasm"
// ns_sentiment.baml:28:26 (string) len=6 "counts"
// ns_sentiment.baml:28:33 (string) len=2 "as"
// ns_sentiment.baml:28:36 (string) len=3 "the"
// ns_sentiment.baml:28:40 (string) len=9 "sentiment"
// ns_sentiment.baml:28:50 (string) len=8 "actually"
// ns_sentiment.baml:28:59 (string) len=9 "expressed"
// ns_sentiment.baml:28:68 (string) len=1 ","
// ns_sentiment.baml:28:70 (string) len=3 "not"
// ns_sentiment.baml:28:74 (string) len=3 "the"
// ns_sentiment.baml:29:9 (string) len=7 "literal"
// ns_sentiment.baml:29:17 (string) len=5 "words"
// ns_sentiment.baml:29:22 (string) len=1 "."
// ns_sentiment.baml:29:24 (string) len=5 "Plain"
// ns_sentiment.baml:29:30 (string) len=10 "statements"
// ns_sentiment.baml:29:41 (string) len=2 "of"
// ns_sentiment.baml:29:44 (string) len=4 "fact"
// ns_sentiment.baml:29:49 (string) len=3 "are"
// ns_sentiment.baml:29:53 (string) len=7 "neutral"
// ns_sentiment.baml:29:60 (string) len=1 "."
// ns_sentiment.baml:31:9 (string) len=4 "Text"
// ns_sentiment.baml:31:13 (string) len=1 ":"
// ns_sentiment.baml:33:15 (property) len=13 "output_format"
// ns_sentiment.baml:34:5 (string) len=1 "`"
