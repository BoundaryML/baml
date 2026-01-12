retry_policy GPT4oRetry {
  max_retries 2
  strategy {
    type exponential_backoff
  }
}

client<llm> GPT35 {
  provider openai
  retry_policy GPT4oRetry
  options {
    model "gpt-4o-mini"
    api_key env.OPENAI_API_KEY
  }
}

class ClassOptionalOutput {
  prop1 string
  prop2 string
}

function FnOutputOptionalClass(input: string) -> ClassOptionalOutput? {
  client GPT35
  prompt #"
    Return a json blob for the following input:
    {{input}}

    {{ctx.output_format()}}

    JSON:
  "#
}


class Blah {
  prop4 string?
}

class ClassOptionalOutput2 {
  prop1 string?
  prop2 string?
  prop3 Blah?
}

function FnOutputOptionalClass2(input: string) -> ClassOptionalOutput2? {
  client GPT35
  prompt #"
    Return a json blob for the following input:
    {{input}}

    {{ctx.output_format()}}

    JSON:
  "#
}

test TestOutputOptionalClass {
  functions [FnOutputOptionalClass2, FnOutputOptionalClass]
  args {
    input "example input"
  }
}
