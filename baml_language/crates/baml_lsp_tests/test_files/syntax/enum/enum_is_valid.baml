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

//----
//- diagnostics
// Error: Expected top-level declaration, found Generator
//    ╭─[ 0:7:1 ]
//    │
//  7 │ generator target{
//    │ ────┬────  
//    │     ╰────── Expected top-level declaration, found Generator
//    │ 
//    │ Note: Error code: E0010
// ───╯
// Error: Expected top-level declaration, found Word
//    ╭─[ 0:7:11 ]
//    │
//  7 │ generator target{
//    │           ───┬──  
//    │              ╰──── Expected top-level declaration, found Word
//    │ 
//    │ Note: Error code: E0010
// ───╯
// Error: Expected top-level declaration, found LBrace
//    ╭─[ 0:7:17 ]
//    │
//  7 │ generator target{
//    │                 ┬  
//    │                 ╰── Expected top-level declaration, found LBrace
//    │ 
//    │ Note: Error code: E0010
// ───╯
// Error: Expected top-level declaration, found Word
//    ╭─[ 0:8:5 ]
//    │
//  8 │     output_type "python/pydantic"
//    │     ─────┬─────  
//    │          ╰─────── Expected top-level declaration, found Word
//    │ 
//    │ Note: Error code: E0010
// ───╯
// Error: Expected top-level declaration, found Quote
//    ╭─[ 0:8:17 ]
//    │
//  8 │     output_type "python/pydantic"
//    │                 ┬  
//    │                 ╰── Expected top-level declaration, found Quote
//    │ 
//    │ Note: Error code: E0010
// ───╯
// Error: Expected top-level declaration, found Word
//    ╭─[ 0:8:18 ]
//    │
//  8 │     output_type "python/pydantic"
//    │                  ───┬──  
//    │                     ╰──── Expected top-level declaration, found Word
//    │ 
//    │ Note: Error code: E0010
// ───╯
// Error: Expected top-level declaration, found Slash
//    ╭─[ 0:8:24 ]
//    │
//  8 │     output_type "python/pydantic"
//    │                        ┬  
//    │                        ╰── Expected top-level declaration, found Slash
//    │ 
//    │ Note: Error code: E0010
// ───╯
// Error: Expected top-level declaration, found Word
//    ╭─[ 0:8:25 ]
//    │
//  8 │     output_type "python/pydantic"
//    │                         ────┬───  
//    │                             ╰───── Expected top-level declaration, found Word
//    │ 
//    │ Note: Error code: E0010
// ───╯
// Error: Expected top-level declaration, found Quote
//    ╭─[ 0:8:33 ]
//    │
//  8 │     output_type "python/pydantic"
//    │                                 ┬  
//    │                                 ╰── Expected top-level declaration, found Quote
//    │ 
//    │ Note: Error code: E0010
// ───╯
// Error: Expected top-level declaration, found RBrace
//    ╭─[ 0:9:1 ]
//    │
//  9 │ }
//    │ ┬  
//    │ ╰── Expected top-level declaration, found RBrace
//    │ 
//    │ Note: Error code: E0010
// ───╯
