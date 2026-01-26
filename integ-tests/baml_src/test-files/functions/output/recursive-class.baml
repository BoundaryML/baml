class Node {
  data int
  next Node?
}

class LinkedList {
  head Node?
  len int
}

client<llm> O1 {
  provider "openai"
  options {
    model "o4-mini"
    default_role "user"
  }
}

function BuildLinkedList(input: int[]) -> LinkedList {
  client O1
  prompt #"
    Build a linked list from the input array of integers.

    INPUT:
    {{ input }}

    {{ ctx.output_format }}    
  "#
}

test TestLinkedList {
  functions [BuildLinkedList]
  args {
    input [1, 2, 3, 4, 5]
  }
}
