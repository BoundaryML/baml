class BinaryNode {
  data int
  left BinaryNode?
  right BinaryNode?
}

function GetTreeLen(input: BinaryNode) -> int {
  client "openai/gpt-4o"
  prompt #"
    Return the number of elements in the input binary tree.

    INPUT:
    {{ input }}

    {{ ctx.output_format }}    
  "#
}

function BuildTree(input: int) -> BinaryNode {
  client "openai/gpt-4o"
  prompt #"
    Return a binary tree with the specified number of elements.

    INPUT:
    {{ input }}

    {{ ctx.output_format }}    
  "#
}

//----
//- diagnostics
// <no-diagnostics-expected>
