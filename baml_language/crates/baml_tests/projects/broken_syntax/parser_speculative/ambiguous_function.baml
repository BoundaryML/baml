// A colon-prefixed field selects the LLM grammar.
function Ambiguous(input: string) -> string {
  client: GPT4
  // Missing prompt field.
}

// A misspelled first field does not select the LLM grammar.
function AnotherLLM(input: string) -> string {
  clent: GPT4
  prompt: `Process: ${input}`
}
