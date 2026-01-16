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
