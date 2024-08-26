
client<llm> FallbackToShorthand {
  provider fallback
  options {
    strategy [
      "openai/does-not-exist",
      "openai/gpt-4o-mini"
    ]
  }
}


function TestFallbackToShorthand(input: string) -> string {
  client FallbackToShorthand
  // TODO make it return the client name instead
  prompt #"
    Say a haiku about {{input}}.
  "#
}

test TestProvider_FallbackToShorthand {
  functions [
    TestFallbackToShorthand
  ]
  args {
    input "Donkey kong and peanut butter"
  }
}
