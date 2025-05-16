enum Test {
    None
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
