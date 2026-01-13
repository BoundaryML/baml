type Graph = map<string, string[]>

function FnOutputTypeAliasMap(m: Graph) -> Graph {
    client "openai/gpt-4o"
    prompt #"
      Return the given Graph back:

      {{ m }}

      {{ ctx.output_format }}
    "#
}

test TestOutputTypeAliasMap {
  functions [FnOutputTypeAliasMap]
  args {
    m {
      "a" ["b", "c"]
    }
  }
}
