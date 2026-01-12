client<llm> GPT4 {
  provider openai
  options {
    model gpt-4o
    api_key env.OPENAI_API_KEY
  }
}

class LiteralClassOne {
  prop "one"
}

class LiteralClassTwo {
  prop "two"
}

function FnInputNamedClassLiteralUnionProp(input: LiteralClassOne | LiteralClassTwo) -> LiteralClassOne | LiteralClassTwo {
  client GPT4
  prompt #"
    Return the same object you were given.
    {{ ctx.output_format() }}

    {{ _.role('user') }}
    {{ input }}
  "#
}

test TestInputNamedClassLiteralUnionProp {
  functions [FnInputNamedClassLiteralUnionProp]
  args {
    input {
      prop "one"
    }
  }
}
