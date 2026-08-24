// Complex expression function that calls LLM functions
function ProcessVideo(transcript: string, title: string?) -> string {
    //# Main Processing Pipeline

    //## Test
    let computed_title = if (!title) {
        //## Generate Title
        GenerateTitle(transcript)
    } else {
        title
    };

    //## Generate Summary
    let summary = SummarizeVideo(transcript);

    //## Create Final Output
    computed_title + ": " + summary
}

// LLM helper functions
function GenerateTitle(transcript: string) -> string {
    client: GPT4
    prompt: `Generate a title from: ${transcript}`
}

function SummarizeVideo(transcript: string) -> string {
    client: GPT4
    prompt: `Summarize: ${transcript}`
}

client GPT4 = openai.ResponsesClient.new(
    model = "gpt-4",
    api_key = env.OPENAI_API_KEY,
);

//----
//- diagnostics
// <no-diagnostics-expected>
