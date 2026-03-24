// A type alias to a type with a check.
type MultipleAttrs = int @assert({{ this > 0 }}) @check(gt_ten, {{ this > 10 }})

// That alias is used as a parameter.
function AliasWithMultipleAttrs(money: MultipleAttrs) -> MultipleAttrs {
    client "openai/gpt-4o"
    prompt #"
      Return the given integer without additional context:

      {{ money }}

      {{ ctx.output_format }}
    "#
}

// error: Error validating: Types with checks are not allowed as function parameters.
//   -->  functions_v2/check_in_parameter_type_alias.baml:5
//    |
//  4 | // That alias is used as a parameter.
//  5 | function AliasWithMultipleAttrs(money: MultipleAttrs) -> MultipleAttrs {
//    |

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
