// This file is just like generator_keywords1.baml, except that the fieldname
// has been changed in order to not collide with the field type `ETA`, and an
// alias is used to render that field as `ETA` in prompts.

generator lang_python {
  output_type python/pydantic
  output_dir "../python"
  version "0.68.0"
}

class ETA {
    thing string
}

class Foo {
    eta ETA? @alias("ETA")
}

//----
//- diagnostics
// Error: Expected top-level declaration, found Generator
//    ╭─[ 0:5:1 ]
//    │
//  5 │ generator lang_python {
//    │ ────┬────  
//    │     ╰────── Expected top-level declaration, found Generator
//    │ 
//    │ Note: Error code: E0010
// ───╯
// Error: Expected top-level declaration, found Word
//    ╭─[ 0:5:11 ]
//    │
//  5 │ generator lang_python {
//    │           ─────┬─────  
//    │                ╰─────── Expected top-level declaration, found Word
//    │ 
//    │ Note: Error code: E0010
// ───╯
// Error: Expected top-level declaration, found LBrace
//    ╭─[ 0:5:23 ]
//    │
//  5 │ generator lang_python {
//    │                       ┬  
//    │                       ╰── Expected top-level declaration, found LBrace
//    │ 
//    │ Note: Error code: E0010
// ───╯
// Error: Expected top-level declaration, found Word
//    ╭─[ 0:6:3 ]
//    │
//  6 │   output_type python/pydantic
//    │   ─────┬─────  
//    │        ╰─────── Expected top-level declaration, found Word
//    │ 
//    │ Note: Error code: E0010
// ───╯
// Error: Expected top-level declaration, found Word
//    ╭─[ 0:6:15 ]
//    │
//  6 │   output_type python/pydantic
//    │               ───┬──  
//    │                  ╰──── Expected top-level declaration, found Word
//    │ 
//    │ Note: Error code: E0010
// ───╯
// Error: Expected top-level declaration, found Slash
//    ╭─[ 0:6:21 ]
//    │
//  6 │   output_type python/pydantic
//    │                     ┬  
//    │                     ╰── Expected top-level declaration, found Slash
//    │ 
//    │ Note: Error code: E0010
// ───╯
// Error: Expected top-level declaration, found Word
//    ╭─[ 0:6:22 ]
//    │
//  6 │   output_type python/pydantic
//    │                      ────┬───  
//    │                          ╰───── Expected top-level declaration, found Word
//    │ 
//    │ Note: Error code: E0010
// ───╯
// Error: Expected top-level declaration, found Word
//    ╭─[ 0:7:3 ]
//    │
//  7 │   output_dir "../python"
//    │   ─────┬────  
//    │        ╰────── Expected top-level declaration, found Word
//    │ 
//    │ Note: Error code: E0010
// ───╯
// Error: Expected top-level declaration, found Quote
//    ╭─[ 0:7:14 ]
//    │
//  7 │   output_dir "../python"
//    │              ┬  
//    │              ╰── Expected top-level declaration, found Quote
//    │ 
//    │ Note: Error code: E0010
// ───╯
// Error: Expected top-level declaration, found Dot
//    ╭─[ 0:7:15 ]
//    │
//  7 │   output_dir "../python"
//    │               ┬  
//    │               ╰── Expected top-level declaration, found Dot
//    │ 
//    │ Note: Error code: E0010
// ───╯
// Error: Expected top-level declaration, found Dot
//    ╭─[ 0:7:16 ]
//    │
//  7 │   output_dir "../python"
//    │                ┬  
//    │                ╰── Expected top-level declaration, found Dot
//    │ 
//    │ Note: Error code: E0010
// ───╯
// Error: Expected top-level declaration, found Slash
//    ╭─[ 0:7:17 ]
//    │
//  7 │   output_dir "../python"
//    │                 ┬  
//    │                 ╰── Expected top-level declaration, found Slash
//    │ 
//    │ Note: Error code: E0010
// ───╯
// Error: Expected top-level declaration, found Word
//    ╭─[ 0:7:18 ]
//    │
//  7 │   output_dir "../python"
//    │                  ───┬──  
//    │                     ╰──── Expected top-level declaration, found Word
//    │ 
//    │ Note: Error code: E0010
// ───╯
// Error: Expected top-level declaration, found Quote
//    ╭─[ 0:7:24 ]
//    │
//  7 │   output_dir "../python"
//    │                        ┬  
//    │                        ╰── Expected top-level declaration, found Quote
//    │ 
//    │ Note: Error code: E0010
// ───╯
// Error: Expected top-level declaration, found Word
//    ╭─[ 0:8:3 ]
//    │
//  8 │   version "0.68.0"
//    │   ───┬───  
//    │      ╰───── Expected top-level declaration, found Word
//    │ 
//    │ Note: Error code: E0010
// ───╯
// Error: Expected top-level declaration, found Quote
//    ╭─[ 0:8:11 ]
//    │
//  8 │   version "0.68.0"
//    │           ┬  
//    │           ╰── Expected top-level declaration, found Quote
//    │ 
//    │ Note: Error code: E0010
// ───╯
// Error: Expected top-level declaration, found FloatLiteral
//    ╭─[ 0:8:12 ]
//    │
//  8 │   version "0.68.0"
//    │            ──┬─  
//    │              ╰─── Expected top-level declaration, found FloatLiteral
//    │ 
//    │ Note: Error code: E0010
// ───╯
// Error: Expected top-level declaration, found Dot
//    ╭─[ 0:8:16 ]
//    │
//  8 │   version "0.68.0"
//    │                ┬  
//    │                ╰── Expected top-level declaration, found Dot
//    │ 
//    │ Note: Error code: E0010
// ───╯
// Error: Expected top-level declaration, found IntegerLiteral
//    ╭─[ 0:8:17 ]
//    │
//  8 │   version "0.68.0"
//    │                 ┬  
//    │                 ╰── Expected top-level declaration, found IntegerLiteral
//    │ 
//    │ Note: Error code: E0010
// ───╯
// Error: Expected top-level declaration, found Quote
//    ╭─[ 0:8:18 ]
//    │
//  8 │   version "0.68.0"
//    │                  ┬  
//    │                  ╰── Expected top-level declaration, found Quote
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
