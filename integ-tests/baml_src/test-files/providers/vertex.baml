function TestVertex(input: string) -> string {
  client Vertex
  prompt #"
    Write a nice short story about {{ input }}. Keep it to 15 words or less.
  "#
}

function TestVertexWithSystemInstructions() -> string {
  client Vertex
  prompt #"{{_.role("system")}} You are a helpful assistant
  {{_.role("user")}} Write a poem about llamas. Keep it to 15 words or less.
  "#
}

test TestVertex {
  functions [TestVertex, TestVertexWithSystemInstructions]
  args {
    input "a cat"

  }
}
