generator lang_python {
  output_type python/pydantic
  output_dir "../python"
  version "0.68.0"
}

class ETA {
    thing string
}

class Foo {
    if string
    ETA ETA?
}

// error: Error validating field `if` in class `if`: Field name is a reserved word in generated python/pydantic clients.
//   -->  class/generator_keywords1.baml:12
//    | 
// 11 | class Foo {
// 12 |     if string
//    | 
// error: Error validating field `ETA` in class `ETA`: When using the python/pydantic generator, a field name must not be exactly equal to the type name. Consider changing the field name and using an alias.
//   -->  class/generator_keywords1.baml:13
//    | 
// 12 |     if string
// 13 |     ETA ETA?
//    |

//----
//- diagnostics
// Error: Expected top-level declaration, found Generator
//    ╭─[ 0:1:1 ]
//    │
//  1 │ generator lang_python {
//    │ ────┬────  
//    │     ╰────── Expected top-level declaration, found Generator
//    │ 
//    │ Note: Error code: E0010
// ───╯
// Error: Expected top-level declaration, found Word
//    ╭─[ 0:1:11 ]
//    │
//  1 │ generator lang_python {
//    │           ─────┬─────  
//    │                ╰─────── Expected top-level declaration, found Word
//    │ 
//    │ Note: Error code: E0010
// ───╯
// Error: Expected top-level declaration, found LBrace
//    ╭─[ 0:1:23 ]
//    │
//  1 │ generator lang_python {
//    │                       ┬  
//    │                       ╰── Expected top-level declaration, found LBrace
//    │ 
//    │ Note: Error code: E0010
// ───╯
// Error: Expected top-level declaration, found Word
//    ╭─[ 0:2:3 ]
//    │
//  2 │   output_type python/pydantic
//    │   ─────┬─────  
//    │        ╰─────── Expected top-level declaration, found Word
//    │ 
//    │ Note: Error code: E0010
// ───╯
// Error: Expected top-level declaration, found Word
//    ╭─[ 0:2:15 ]
//    │
//  2 │   output_type python/pydantic
//    │               ───┬──  
//    │                  ╰──── Expected top-level declaration, found Word
//    │ 
//    │ Note: Error code: E0010
// ───╯
// Error: Expected top-level declaration, found Slash
//    ╭─[ 0:2:21 ]
//    │
//  2 │   output_type python/pydantic
//    │                     ┬  
//    │                     ╰── Expected top-level declaration, found Slash
//    │ 
//    │ Note: Error code: E0010
// ───╯
// Error: Expected top-level declaration, found Word
//    ╭─[ 0:2:22 ]
//    │
//  2 │   output_type python/pydantic
//    │                      ────┬───  
//    │                          ╰───── Expected top-level declaration, found Word
//    │ 
//    │ Note: Error code: E0010
// ───╯
// Error: Expected top-level declaration, found Word
//    ╭─[ 0:3:3 ]
//    │
//  3 │   output_dir "../python"
//    │   ─────┬────  
//    │        ╰────── Expected top-level declaration, found Word
//    │ 
//    │ Note: Error code: E0010
// ───╯
// Error: Expected top-level declaration, found Quote
//    ╭─[ 0:3:14 ]
//    │
//  3 │   output_dir "../python"
//    │              ┬  
//    │              ╰── Expected top-level declaration, found Quote
//    │ 
//    │ Note: Error code: E0010
// ───╯
// Error: Expected top-level declaration, found Dot
//    ╭─[ 0:3:15 ]
//    │
//  3 │   output_dir "../python"
//    │               ┬  
//    │               ╰── Expected top-level declaration, found Dot
//    │ 
//    │ Note: Error code: E0010
// ───╯
// Error: Expected top-level declaration, found Dot
//    ╭─[ 0:3:16 ]
//    │
//  3 │   output_dir "../python"
//    │                ┬  
//    │                ╰── Expected top-level declaration, found Dot
//    │ 
//    │ Note: Error code: E0010
// ───╯
// Error: Expected top-level declaration, found Slash
//    ╭─[ 0:3:17 ]
//    │
//  3 │   output_dir "../python"
//    │                 ┬  
//    │                 ╰── Expected top-level declaration, found Slash
//    │ 
//    │ Note: Error code: E0010
// ───╯
// Error: Expected top-level declaration, found Word
//    ╭─[ 0:3:18 ]
//    │
//  3 │   output_dir "../python"
//    │                  ───┬──  
//    │                     ╰──── Expected top-level declaration, found Word
//    │ 
//    │ Note: Error code: E0010
// ───╯
// Error: Expected top-level declaration, found Quote
//    ╭─[ 0:3:24 ]
//    │
//  3 │   output_dir "../python"
//    │                        ┬  
//    │                        ╰── Expected top-level declaration, found Quote
//    │ 
//    │ Note: Error code: E0010
// ───╯
// Error: Expected top-level declaration, found Word
//    ╭─[ 0:4:3 ]
//    │
//  4 │   version "0.68.0"
//    │   ───┬───  
//    │      ╰───── Expected top-level declaration, found Word
//    │ 
//    │ Note: Error code: E0010
// ───╯
// Error: Expected top-level declaration, found Quote
//    ╭─[ 0:4:11 ]
//    │
//  4 │   version "0.68.0"
//    │           ┬  
//    │           ╰── Expected top-level declaration, found Quote
//    │ 
//    │ Note: Error code: E0010
// ───╯
// Error: Expected top-level declaration, found FloatLiteral
//    ╭─[ 0:4:12 ]
//    │
//  4 │   version "0.68.0"
//    │            ──┬─  
//    │              ╰─── Expected top-level declaration, found FloatLiteral
//    │ 
//    │ Note: Error code: E0010
// ───╯
// Error: Expected top-level declaration, found Dot
//    ╭─[ 0:4:16 ]
//    │
//  4 │   version "0.68.0"
//    │                ┬  
//    │                ╰── Expected top-level declaration, found Dot
//    │ 
//    │ Note: Error code: E0010
// ───╯
// Error: Expected top-level declaration, found IntegerLiteral
//    ╭─[ 0:4:17 ]
//    │
//  4 │   version "0.68.0"
//    │                 ┬  
//    │                 ╰── Expected top-level declaration, found IntegerLiteral
//    │ 
//    │ Note: Error code: E0010
// ───╯
// Error: Expected top-level declaration, found Quote
//    ╭─[ 0:4:18 ]
//    │
//  4 │   version "0.68.0"
//    │                  ┬  
//    │                  ╰── Expected top-level declaration, found Quote
//    │ 
//    │ Note: Error code: E0010
// ───╯
// Error: Expected top-level declaration, found RBrace
//    ╭─[ 0:5:1 ]
//    │
//  5 │ }
//    │ ┬  
//    │ ╰── Expected top-level declaration, found RBrace
//    │ 
//    │ Note: Error code: E0010
// ───╯
// Error: Expected Unexpected token in class body, found If
//     ╭─[ 0:12:5 ]
//     │
//  12 │     if string
//     │     ─┬  
//     │      ╰── Expected Unexpected token in class body, found If
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected type, found Question
//     ╭─[ 0:13:12 ]
//     │
//  13 │     ETA ETA?
//     │            ┬  
//     │            ╰── Expected type, found Question
//     │ 
//     │ Note: Error code: E0010
// ────╯
