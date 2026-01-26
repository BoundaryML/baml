// LLM functions with prompts
function GeneratePoem(topic: string) -> string {
    client "openai/gpt-4o-mini"
    prompt #"Write a short poem about {{ topic }}"#
}

function ClassifyText(text: string) -> string {
    client "openai/gpt-4o-mini"
    prompt #"
        Classify the following text as positive, negative, or neutral:
        {{ text }}

        Return just the classification.
    "#
}

function ExtractInfo(document: string, field: string) -> string {
    client "openai/gpt-4o"
    prompt #"
        From the following document, extract the {{ field }}:

        Document: {{ document }}

        {{ field }}:
    "#
}

enum Sentiment {
    POSITIVE
    NEGATIVE
    NEUTRAL
}

function AnalyzeSentiment(text: string) -> Sentiment {
    client "openai/gpt-4o-mini"
    prompt #"
        Analyze the sentiment of this text: {{ text }}

        Respond with exactly one of: POSITIVE, NEGATIVE, NEUTRAL
    "#
}

// function GeneratePoem(topic) -> string {
// client "openai/gpt-4o-mini"
//   prompt "Write a short poem about {{ topic }}"
// }
//
// function ClassifyText(text) -> string {
// client "openai/gpt-4o-mini"
//   prompt "Classify the following text as positive, negative, or neutral:\n{{ text }}\n\nReturn just the classification."
// }
//
// function ExtractInfo(document, field) -> string {
// client "openai/gpt-4o"
//   prompt "From the following document, extract the {{ field }}:\n\nDocument: {{ document }}\n\n{{ field }}:"
// }
//
// function AnalyzeSentiment(text) -> Sentiment {
// client "openai/gpt-4o-mini"
//   prompt "Analyze the sentiment of this text: {{ text }}\n\nRespond with exactly one of: POSITIVE, NEGATIVE, NEUTRAL"
// }
//
// enum Sentiment {
//   POSITIVE
//   NEGATIVE
//   NEUTRAL
// }
