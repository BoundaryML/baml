// This could initially look like either type
function Ambiguous(input string) -> string {
  client GPT4  // This keyword determines it's an LLM function
  // Even though it starts like it could be expression function
}

// This one has a typo but intent is clear
function AnotherLLM(input string) -> string {
  clent GPT4  // Typo in "client"
  prompt "Process: {{ input }}"  // "prompt" keyword helps disambiguate
}
