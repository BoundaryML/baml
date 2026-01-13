client<llm> GPT4o {
  provider openai
  options {
    model gpt-4o
    api_key env.OPENAI_API_KEY
  }
}

class LinkedListAliasNode {
    value int
    next LinkedListAliasNode?
}

type LinkedListAlias = LinkedListAliasNode

// Class that points to an alias that points to a recursive type.
class ClassToRecAlias {
    list LinkedListAlias
}

function FnOutputRecursiveTypeAliasClass(cls: ClassToRecAlias) -> ClassToRecAlias {
    client "openai/gpt-4o"
    prompt #"
      Return the given object back:

      {{ cls }}

      {{ ctx.output_format }}
    "#
}

test TestOutputRecursiveTypeAliasClass {
  functions [FnOutputRecursiveTypeAliasClass]
  args {
    cls {
      list {
        value 1
      }
    }
  }
}
