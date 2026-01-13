type Primitive = int | string | bool | float

type List = string[]

type Graph = map<string, string[]>

type Combination = Primitive | List | Graph

function FnOutputTypeAliasNested(c: Combination) -> Combination {
    client "openai/gpt-4o"
    prompt #"
      Return the given value back:

      {{ c }}

      {{ ctx.output_format }}
    "#
}

test TestOutputTypeAliasNested {
  functions [FnOutputTypeAliasNested]
  args {
    c "hello"
  }
}
