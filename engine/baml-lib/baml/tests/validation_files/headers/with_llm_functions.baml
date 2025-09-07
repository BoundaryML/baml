// Test LLM functions (these should work with client/prompt)
function GenerateTitle(transcript: string) -> string {
    client GPT4
    prompt #"Generate a title from: {{ transcript }}"#
}

function SummarizeVideo(transcript: string) -> string {
    client GPT4
    prompt #"Summarize: {{ transcript }}"#
}

function CombineOutputs(email: string, x: string, linkedin: string) -> string {
    client GPT4
    prompt #"Combine all outputs"#
}

client<llm> GPT4 {
    provider openai
    options {
        model gpt-4
        api_key env.OPENAI_API_KEY
    }
}

// warning: Workflow functions are experimental, and will break in the future.
//   -->  headers/mdx_headers_with_llm_functions.baml:2
//    | 
//  1 | // Test LLM functions (these should work with client/prompt)
//  2 | function GenerateTitle(transcript: string) -> string {
//    | 
// warning: Workflow functions are experimental, and will break in the future.
//   -->  headers/mdx_headers_with_llm_functions.baml:7
//    | 
//  6 | 
//  7 | function SummarizeVideo(transcript: string) -> string {
//    | 
// warning: Workflow functions are experimental, and will break in the future.
//   -->  headers/mdx_headers_with_llm_functions.baml:12
//    | 
// 11 | 
// 12 | function CombineOutputs(email: string, x: string, linkedin: string) -> string {
//    | 


