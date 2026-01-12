client<llm> GPT4 {
  provider openai
  options {
    model gpt-4o
    api_key env.OPENAI_API_KEY
  }
}

class LiteralClassHello {
  prop "hello"
}

function FnInputNamedClassLiteralProp(input: LiteralClassHello) -> LiteralClassHello {
  client GPT4
  prompt #"
    Return the same object you were given.
    {{ ctx.output_format() }}
  "#
}

test TestInputNamedClassLiteralProp {
  functions [FnInputNamedClassLiteralProp]
  args {
    input {
      prop "hello"
    }
  }
}
