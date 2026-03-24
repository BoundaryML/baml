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
// Error: unreachable arm
//      ╭─[ llm_types.baml:159:17 ]
//      │
//  159 │             _ => { continue; }
//      │                 ───────┬──────  
//      │                        ╰──────── unreachable arm
//      │ 
//      │ Note: Error code: E0001
// ─────╯
// Error: unreachable arm
//      ╭─[ llm_types.baml:189:17 ]
//      │
//  189 │ ╭─▶             _ => {
//      ┆ ┆   
//  194 │ ├─▶             }
//      │ │                   
//      │ ╰─────────────────── unreachable arm
//      │     
//      │     Note: Error code: E0001
// ─────╯
// Error: unreachable arm
//      ╭─[ llm_types.baml:208:17 ]
//      │
//  208 │             _ => { continue; }
//      │                 ───────┬──────  
//      │                        ╰──────── unreachable arm
//      │ 
//      │ Note: Error code: E0001
// ─────╯
