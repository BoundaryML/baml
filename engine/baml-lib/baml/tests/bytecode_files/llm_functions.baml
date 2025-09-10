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
// Enum std::HttpMethod
// Enum Sentiment
// Function: std.Array.len
//
// Function: std.Map.len
//
// Function: std.Map.contains
//
// Function: std.media.image.from_url
//
// Function: std.media.audio.from_url
//
// Function: std.media.video.from_url
//
// Function: std.media.pdf.from_url
//
// Function: std.media.image.from_base64
//
// Function: std.media.audio.from_base64
//
// Function: std.media.video.from_base64
//
// Function: std.media.pdf.from_base64
//
// Function: std.media.image.is_url
//
// Function: std.media.video.is_url
//
// Function: std.media.audio.is_url
//
// Function: std.media.pdf.is_url
//
// Function: std.media.image.is_base64
//
// Function: std.media.video.is_base64
//
// Function: std.media.audio.is_base64
//
// Function: std.media.pdf.is_base64
//
// Function: std.media.image.as_url
//
// Function: std.media.video.as_url
//
// Function: std.media.audio.as_url
//
// Function: std.media.pdf.as_url
//
// Function: std.media.image.as_base64
//
// Function: std.media.video.as_base64
//
// Function: std.media.audio.as_base64
//
// Function: std.media.pdf.as_base64
//
// Function: std.media.image.mime
//
// Function: std.media.video.mime
//
// Function: std.media.audio.mime
//
// Function: std.media.pdf.mime
