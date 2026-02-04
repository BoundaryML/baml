class Person {
    name string
    @@<[CURSOR]
}

//----
//- diagnostics
// (parse error expected - incomplete block attribute)
// Error: Expected attribute name, found '}'
//    ╭─[ block_attribute.baml:4:1 ]
//    │
//  4 │ }
//    │ ┬  
//    │ ╰── Expected attribute name, found '}'
//    │ 
//    │ Note: Error code: E0010
// ───╯
