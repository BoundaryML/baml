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
// error: field 'field' is missing a type annotation
//     ╭─[ type_position.baml:11:5 ]
//     │
//  11 │     field :
//     │     ──┬──  
//     │       ╰──── field 'field' is missing a type annotation
//     │ 
//     │ Note: Error code: E0010
// ────╯
//
//- completions
// (new pipeline detects top-level context here instead of type position)
// SHOULD_CONTAIN: class, enum, function
