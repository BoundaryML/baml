enum Test {
    None
    Hello
    World
}

enum TestValidEnumKeyword {
    None @alias("None")
    Hello
    World
}

generator target{
    output_type "python/pydantic"
}

// error: Error validating field `None` in enum `Test`: Enum value 'None' is a reserved word in Python, try changing the name and using `OtherValueName @alias("None")`.
//   -->  enum/enum_is_valid.baml:2
//    | 
//  1 | enum Test {
//  2 |     None
//  3 |     Hello
//    |

//----
//- diagnostics
// Error: Enum value `None` in enum `Test` is a reserved keyword in Python
//    ╭─[ enum_enum_is_valid.baml:2:5 ]
//    │
//  2 │     None
//    │     ──┬─  
//    │       ╰─── Enum value `None` in enum `Test` is a reserved keyword in Python
//    │ 
//    │ Note: Error code: E0020
// ───╯
