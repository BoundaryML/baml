template_string FunctionWithBadParams(
    param: Unknown,
    param2: Unknown2[],
    param3: string
) #"
    {{ param.foo }}
    {{ param2[0].doc }}
    {{ param3 }}
"#

// warning: 'param' is undefined, expected class
//   -->  template_string/invalid.baml:6
//    |
//  5 | ) #"
//  6 |     {{ param.foo }}
//    |
// warning: 'param' is undefined, expected class
//   -->  template_string/invalid.baml:6
//    |
//  5 | ) #"
//  6 |     {{ param.foo }}
//    |
// error: Type `Unknown` does not exist. Did you mean one of these: `int`, `float`, `bool`, `string`, `true`, `false`?
//   -->  template_string/invalid.baml:2
//    |
//  1 | template_string FunctionWithBadParams(
//  2 |     param: Unknown,
//    |
// error: Type `Unknown2` does not exist. Did you mean one of these: `string`, `int`, `float`, `bool`, `true`, `false`?
//   -->  template_string/invalid.baml:3
//    |
//  2 |     param: Unknown,
//  3 |     param2: Unknown2[],
//    |

//----
//- diagnostics
// Error: Unknown type `Unknown`
//    ╭─[ template_string_invalid.baml:2:11 ]
//    │
//  2 │     param: Unknown,
//    │           ────┬───  
//    │               ╰───── Unknown type `Unknown`
//    │ 
//    │ Note: Error code: E0002
// ───╯
// Error: Unknown type `Unknown2`
//    ╭─[ template_string_invalid.baml:3:12 ]
//    │
//  3 │     param2: Unknown2[],
//    │            ─────┬─────  
//    │                 ╰─────── Unknown type `Unknown2`
//    │ 
//    │ Note: Error code: E0002
// ───╯
