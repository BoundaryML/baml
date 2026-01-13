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

function FnOutputRecursiveTypeAliases(data: LinkedListAlias) -> LinkedListAlias {
    client "openai/gpt-4o"
    prompt #"
      Return the given linked list back:

      {{ data }}

      {{ ctx.output_format() }}
    "#
}

// Class that points to an alias that points to a recursive type.
class ClassToRecAlias {
    list LinkedListAlias
}

function FnOutputRecursiveTypeAliases2(cls: ClassToRecAlias) -> ClassToRecAlias {
    client "openai/gpt-4o"
    prompt #"
      Return the given object back:

      {{ cls }}

      {{ ctx.output_format() }}
    "#
}

// This is tricky cause this class should be hoisted, but classes and aliases
// are two different types in the AST. This test will make sure they can interop.
class NodeWithAliasIndirection {
    value int
    next NodeIndirection?
}

type NodeIndirection = NodeWithAliasIndirection

function FnOutputRecursiveTypeAliases3(cls: NodeWithAliasIndirection) -> NodeWithAliasIndirection {
    client "openai/gpt-4o"
    prompt #"
      Return the given object back:

      {{ cls }}

      {{ ctx.output_format() }}
    "#
}

type JsonEntry = SimpleTag | JsonTemplate

type JsonTemplate = map<string, JsonEntry>

class SimpleTag {
  field string
}

function FnOutputRecursiveTypeAliases4(s: string) -> JsonTemplate {
  client GPT4o
  prompt #"
    {{ _.role("user") }}

    Extract info from this string exactly as it is:
    {{ s }}

    {{ ctx.output_format() }}
  "#
}

test TestOutputRecursiveTypeAliases {
  functions [FnOutputRecursiveTypeAliases, FnOutputRecursiveTypeAliases2, FnOutputRecursiveTypeAliases3, FnOutputRecursiveTypeAliases4]
  args {
    s "Hello, world!"
  }
}
