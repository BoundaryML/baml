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
    prompt: #"
        Classify the sentiment of the text below as positive, negative, or
        neutral. Sarcasm counts as the sentiment actually expressed, not the
        literal words. Plain statements of fact are neutral.

        Text: {{ text }}

        {{ ctx.output_format }}
    "#
}
