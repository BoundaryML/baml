client<llm> GPT4o {
  provider openai
  options {
    model gpt-4o
    api_key env.OPENAI_API_KEY
  }
}

type JsonEntry = SimpleTag | JsonTemplate

type JsonTemplate = map<string, JsonEntry>

class SimpleTag {
  field string
}

function FnOutputRecursiveTypeAliasJsonTemplate(s: string) -> JsonTemplate {
  client GPT4o
  prompt #"
    {{ _.role("user") }}

    Extract info from this string exactly as it is:
    {{ s }}

    {{ ctx.output_format }}
  "#
}

test TestOutputRecursiveTypeAliasJsonTemplate {
  functions [FnOutputRecursiveTypeAliasJsonTemplate]
  args {
    s "Hello, world!"
  }
}
