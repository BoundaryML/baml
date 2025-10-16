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
    client GPT4
    prompt #"Generate a title from: {{ transcript }}"#
}

function SummarizeVideo(transcript: string) -> string {
    client GPT4
    prompt #"Summarize: {{ transcript }}"#
}

client<llm> GPT4 {
    provider openai
    options {
        model gpt-4
        api_key env.OPENAI_API_KEY
    }
}

// warning: Workflow functions are experimental, and will break in the future.
//   -->  headers/mdx_headers_complex_workflow.baml:2
//    |
//  1 | // Complex expression function that calls LLM functions
//  2 | function ProcessVideo(transcript: string, title: string?) -> string {
//    |
// warning: Workflow functions are experimental, and will break in the future.
//   -->  headers/mdx_headers_complex_workflow.baml:20
//    |
// 19 | // LLM helper functions
// 20 | function GenerateTitle(transcript: string) -> string {
//    |
// warning: Workflow functions are experimental, and will break in the future.
//   -->  headers/mdx_headers_complex_workflow.baml:25
//    |
// 24 |
// 25 | function SummarizeVideo(transcript: string) -> string {
//    |
// error: Error validating: Type mismatch in if
//   -->  headers/mdx_headers_complex_workflow.baml:5
//    |
//  4 |
//  5 |     let computed_title = if !title {
//  6 |         ## Generate Title
//  7 |         GenerateTitle(transcript)
//  8 |     } else {
//  9 |         title
// 10 |     };
//    |
