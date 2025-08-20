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

// Function: GeneratePoem
//
// Function: ClassifyText
//
// Function: AnalyzeSentiment
//
// Class: std::Request with 3 fields
// Function: len
