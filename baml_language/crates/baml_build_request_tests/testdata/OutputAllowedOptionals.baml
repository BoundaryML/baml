client<llm> GPT4 {
  provider openai
  options {
    model gpt-4o
    api_key env.OPENAI_API_KEY
  }
}

class OptionalListAndMap {
  p string[]?
  q map<string, string>?
}

function FnOutputAllowedOptionals(optionals: OptionalListAndMap) -> OptionalListAndMap {
  client GPT4
  prompt #"
    Return the given input:

    {{optionals}}

    {{ctx.output_format}}
  "#
}

test TestOutputAllowedOptionals {
  functions [FnOutputAllowedOptionals]
  args {
    optionals {
      p ["test"]
      q { "key" "value" }
    }
  }
}
