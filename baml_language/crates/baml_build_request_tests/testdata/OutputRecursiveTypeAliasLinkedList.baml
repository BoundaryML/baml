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

// Simple alias that points to recursive type.
type LinkedListAlias = LinkedListAliasNode

function FnOutputRecursiveTypeAliasLinkedList(data: LinkedListAlias) -> LinkedListAlias {
    client "openai/gpt-4o"
    prompt #"
      Return the given linked list back:

      {{ data }}

      {{ ctx.output_format }}
    "#
}

test TestOutputRecursiveTypeAliasLinkedList {
  functions [FnOutputRecursiveTypeAliasLinkedList]
  args {
    data {
      value 1
    }
  }
}
