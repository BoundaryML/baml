function LLM(x: int) -> int {
    client GPT4o
    prompt #"Return {{ x }} {{ ctx.output_format}}"#
}

function UseLLM(x: int) -> int {
    LLM(x)
}

client<llm> GPT4o {
  provider openai
  options {
    model gpt-4o
    api_key env.OPENAI_API_KEY
  }
}

// warning: Workflow functions are experimental, and will break in the future.
//   -->  expr/mixed_pipeline.baml:6
//    | 
//  5 | 
//  6 | function UseLLM(x: int) -> int {
//    | 
