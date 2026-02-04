class MyClass {
    value string
}

enum MyEnum {
    A
    B
}

class Test {
    field :<[CURSOR]
}

//----
//- diagnostics
// (parse error expected - incomplete field)
// Error: field 'field' is missing a type annotation
//     ╭─[ type_position.baml:11:5 ]
//     │
//  11 │     field :
//     │     ──┬──  
//     │       ╰──── field 'field' is missing a type annotation
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected Unexpected token in class body, found ':'
//     ╭─[ type_position.baml:11:11 ]
//     │
//  11 │     field :
//     │           ┬  
//     │           ╰── Expected Unexpected token in class body, found ':'
//     │ 
//     │ Note: Error code: E0010
// ────╯
//
//- completions
// SHOULD_CONTAIN: int, float, string, bool, null, image, audio, MyClass, MyEnum
