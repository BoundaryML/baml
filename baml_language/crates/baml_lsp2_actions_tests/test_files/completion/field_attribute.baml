class Person {
    name @<[CURSOR] string
}

//----
//- diagnostics
// (parse error expected - incomplete attribute)
// error: field 'name' is missing a type annotation
//    ╭─[ field_attribute.baml:2:5 ]
//    │
//  2 │     name @ string
//    │     ──┬─  
//    │       ╰─── field 'name' is missing a type annotation
//    │ 
//    │ Note: Error code: E0010
// ───╯
// error: Expected Unexpected token in class body, found '@'
//    ╭─[ field_attribute.baml:2:10 ]
//    │
//  2 │     name @ string
//    │          ┬  
//    │          ╰── Expected Unexpected token in class body, found '@'
//    │ 
//    │ Note: Error code: E0010
// ───╯
// error: field 'string' is missing a type annotation
//    ╭─[ field_attribute.baml:2:12 ]
//    │
//  2 │     name @ string
//    │            ───┬──  
//    │               ╰──── field 'string' is missing a type annotation
//    │ 
//    │ Note: Error code: E0010
// ───╯
