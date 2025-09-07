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