class OkClass {
  field string
}

enum OkEnum {
  A
  B
}

classs WrongClass {
  field string
}

random_keyword WrongEnum {
  A
  B
}

// error: Error validating: Unexpected keyword 'classs' in type definition. Use 'class' or 'enum'.
//   -->  class/invalid_keyword_in_type_def.baml:10
//    | 
//  9 | 
// 10 | classs WrongClass {
//    | 
// error: Error validating: Unexpected keyword 'random_keyword' in type definition. Use 'class' or 'enum'.
//   -->  class/invalid_keyword_in_type_def.baml:14
//    | 
// 13 | 
// 14 | random_keyword WrongEnum {
//    |

//----
//- diagnostics
// Error: Expected top-level declaration, found Word
//     ╭─[ 0:10:1 ]
//     │
//  10 │ classs WrongClass {
//     │ ───┬──  
//     │    ╰──── Expected top-level declaration, found Word
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found Word
//     ╭─[ 0:10:8 ]
//     │
//  10 │ classs WrongClass {
//     │        ─────┬────  
//     │             ╰────── Expected top-level declaration, found Word
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found LBrace
//     ╭─[ 0:10:19 ]
//     │
//  10 │ classs WrongClass {
//     │                   ┬  
//     │                   ╰── Expected top-level declaration, found LBrace
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found Word
//     ╭─[ 0:11:3 ]
//     │
//  11 │   field string
//     │   ──┬──  
//     │     ╰──── Expected top-level declaration, found Word
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found Word
//     ╭─[ 0:11:9 ]
//     │
//  11 │   field string
//     │         ───┬──  
//     │            ╰──── Expected top-level declaration, found Word
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found RBrace
//     ╭─[ 0:12:1 ]
//     │
//  12 │ }
//     │ ┬  
//     │ ╰── Expected top-level declaration, found RBrace
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found Word
//     ╭─[ 0:14:1 ]
//     │
//  14 │ random_keyword WrongEnum {
//     │ ───────┬──────  
//     │        ╰──────── Expected top-level declaration, found Word
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found Word
//     ╭─[ 0:14:16 ]
//     │
//  14 │ random_keyword WrongEnum {
//     │                ────┬────  
//     │                    ╰────── Expected top-level declaration, found Word
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found LBrace
//     ╭─[ 0:14:26 ]
//     │
//  14 │ random_keyword WrongEnum {
//     │                          ┬  
//     │                          ╰── Expected top-level declaration, found LBrace
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found Word
//     ╭─[ 0:15:3 ]
//     │
//  15 │   A
//     │   ┬  
//     │   ╰── Expected top-level declaration, found Word
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found Word
//     ╭─[ 0:16:3 ]
//     │
//  16 │   B
//     │   ┬  
//     │   ╰── Expected top-level declaration, found Word
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found RBrace
//     ╭─[ 0:17:1 ]
//     │
//  17 │ }
//     │ ┬  
//     │ ╰── Expected top-level declaration, found RBrace
//     │ 
//     │ Note: Error code: E0010
// ────╯
