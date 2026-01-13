client<llm> GPT4o {
  provider openai
  options {
    model gpt-4o
    api_key env.OPENAI_API_KEY
  }
}

// This is tricky cause this class should be hoisted, but classes and aliases
// are two different types in the AST. This test will make sure they can interop.
class NodeWithAliasIndirection {
    value int
    next NodeIndirection?
}

type NodeIndirection = NodeWithAliasIndirection

function FnOutputRecursiveTypeAliasIndirection(cls: NodeWithAliasIndirection) -> NodeWithAliasIndirection {
    client "openai/gpt-4o"
    prompt #"
      Return the given object back:

      {{ cls }}

      {{ ctx.output_format }}
    "#
}

test TestOutputRecursiveTypeAliasIndirection {
  functions [FnOutputRecursiveTypeAliasIndirection]
  args {
    cls {
      value 1
    }
  }
}
