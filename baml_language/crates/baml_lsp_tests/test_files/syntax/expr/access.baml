class Foo {
    name string
    age int
}

let foo = Foo {
    name: "John",
    age: 2,
};
let name = foo.name;
let misspelling = foo.names;
let nonsense = foo.name.field;

let array_access_1 = [1, 2, 3][0];
let array_access_2 = [1,2, 3]["hi"];
let array_access_3 = [1,2, 3][foo.age];
let array_access_4 = [1,2, 3][foo.name];

// error: Error validating: Class Foo has no field names
//   -->  expr/access.baml:11
//    | 
// 10 | let name = foo.name;
// 11 | let misspelling = foo.names;
//    | 
// error: Error validating: Can only access fields on class instances
//   -->  expr/access.baml:12
//    | 
// 11 | let misspelling = foo.names;
// 12 | let nonsense = foo.name.field;
//    | 
// error: Error validating: Array index must be integer
//   -->  expr/access.baml:15
//    | 
// 14 | let array_access_1 = [1, 2, 3][0];
// 15 | let array_access_2 = [1,2, 3]["hi"];
//    | 
// error: Error validating: Array index must be integer
//   -->  expr/access.baml:17
//    | 
// 16 | let array_access_3 = [1,2, 3][foo.age];
// 17 | let array_access_4 = [1,2, 3][foo.name];
//    |

//----
//- diagnostics
// Error: Expected top-level declaration, found Let
//    ╭─[ 0:6:1 ]
//    │
//  6 │ let foo = Foo {
//    │ ─┬─  
//    │  ╰─── Expected top-level declaration, found Let
//    │ 
//    │ Note: Error code: E0010
// ───╯
// Error: Expected top-level declaration, found Word
//    ╭─[ 0:6:5 ]
//    │
//  6 │ let foo = Foo {
//    │     ─┬─  
//    │      ╰─── Expected top-level declaration, found Word
//    │ 
//    │ Note: Error code: E0010
// ───╯
// Error: Expected top-level declaration, found Equals
//    ╭─[ 0:6:9 ]
//    │
//  6 │ let foo = Foo {
//    │         ┬  
//    │         ╰── Expected top-level declaration, found Equals
//    │ 
//    │ Note: Error code: E0010
// ───╯
// Error: Expected top-level declaration, found Word
//    ╭─[ 0:6:11 ]
//    │
//  6 │ let foo = Foo {
//    │           ─┬─  
//    │            ╰─── Expected top-level declaration, found Word
//    │ 
//    │ Note: Error code: E0010
// ───╯
// Error: Expected top-level declaration, found LBrace
//    ╭─[ 0:6:15 ]
//    │
//  6 │ let foo = Foo {
//    │               ┬  
//    │               ╰── Expected top-level declaration, found LBrace
//    │ 
//    │ Note: Error code: E0010
// ───╯
// Error: Expected top-level declaration, found Word
//    ╭─[ 0:7:5 ]
//    │
//  7 │     name: "John",
//    │     ──┬─  
//    │       ╰─── Expected top-level declaration, found Word
//    │ 
//    │ Note: Error code: E0010
// ───╯
// Error: Expected top-level declaration, found Colon
//    ╭─[ 0:7:9 ]
//    │
//  7 │     name: "John",
//    │         ┬  
//    │         ╰── Expected top-level declaration, found Colon
//    │ 
//    │ Note: Error code: E0010
// ───╯
// Error: Expected top-level declaration, found Quote
//    ╭─[ 0:7:11 ]
//    │
//  7 │     name: "John",
//    │           ┬  
//    │           ╰── Expected top-level declaration, found Quote
//    │ 
//    │ Note: Error code: E0010
// ───╯
// Error: Expected top-level declaration, found Word
//    ╭─[ 0:7:12 ]
//    │
//  7 │     name: "John",
//    │            ──┬─  
//    │              ╰─── Expected top-level declaration, found Word
//    │ 
//    │ Note: Error code: E0010
// ───╯
// Error: Expected top-level declaration, found Quote
//    ╭─[ 0:7:16 ]
//    │
//  7 │     name: "John",
//    │                ┬  
//    │                ╰── Expected top-level declaration, found Quote
//    │ 
//    │ Note: Error code: E0010
// ───╯
// Error: Expected top-level declaration, found Comma
//    ╭─[ 0:7:17 ]
//    │
//  7 │     name: "John",
//    │                 ┬  
//    │                 ╰── Expected top-level declaration, found Comma
//    │ 
//    │ Note: Error code: E0010
// ───╯
// Error: Expected top-level declaration, found Word
//    ╭─[ 0:8:5 ]
//    │
//  8 │     age: 2,
//    │     ─┬─  
//    │      ╰─── Expected top-level declaration, found Word
//    │ 
//    │ Note: Error code: E0010
// ───╯
// Error: Expected top-level declaration, found Colon
//    ╭─[ 0:8:8 ]
//    │
//  8 │     age: 2,
//    │        ┬  
//    │        ╰── Expected top-level declaration, found Colon
//    │ 
//    │ Note: Error code: E0010
// ───╯
// Error: Expected top-level declaration, found IntegerLiteral
//    ╭─[ 0:8:10 ]
//    │
//  8 │     age: 2,
//    │          ┬  
//    │          ╰── Expected top-level declaration, found IntegerLiteral
//    │ 
//    │ Note: Error code: E0010
// ───╯
// Error: Expected top-level declaration, found Comma
//    ╭─[ 0:8:11 ]
//    │
//  8 │     age: 2,
//    │           ┬  
//    │           ╰── Expected top-level declaration, found Comma
//    │ 
//    │ Note: Error code: E0010
// ───╯
// Error: Expected top-level declaration, found RBrace
//    ╭─[ 0:9:1 ]
//    │
//  9 │ };
//    │ ┬  
//    │ ╰── Expected top-level declaration, found RBrace
//    │ 
//    │ Note: Error code: E0010
// ───╯
// Error: Expected top-level declaration, found Semicolon
//    ╭─[ 0:9:2 ]
//    │
//  9 │ };
//    │  ┬  
//    │  ╰── Expected top-level declaration, found Semicolon
//    │ 
//    │ Note: Error code: E0010
// ───╯
// Error: Expected top-level declaration, found Let
//     ╭─[ 0:10:1 ]
//     │
//  10 │ let name = foo.name;
//     │ ─┬─  
//     │  ╰─── Expected top-level declaration, found Let
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found Word
//     ╭─[ 0:10:5 ]
//     │
//  10 │ let name = foo.name;
//     │     ──┬─  
//     │       ╰─── Expected top-level declaration, found Word
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found Equals
//     ╭─[ 0:10:10 ]
//     │
//  10 │ let name = foo.name;
//     │          ┬  
//     │          ╰── Expected top-level declaration, found Equals
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found Word
//     ╭─[ 0:10:12 ]
//     │
//  10 │ let name = foo.name;
//     │            ─┬─  
//     │             ╰─── Expected top-level declaration, found Word
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found Dot
//     ╭─[ 0:10:15 ]
//     │
//  10 │ let name = foo.name;
//     │               ┬  
//     │               ╰── Expected top-level declaration, found Dot
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found Word
//     ╭─[ 0:10:16 ]
//     │
//  10 │ let name = foo.name;
//     │                ──┬─  
//     │                  ╰─── Expected top-level declaration, found Word
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found Semicolon
//     ╭─[ 0:10:20 ]
//     │
//  10 │ let name = foo.name;
//     │                    ┬  
//     │                    ╰── Expected top-level declaration, found Semicolon
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found Let
//     ╭─[ 0:11:1 ]
//     │
//  11 │ let misspelling = foo.names;
//     │ ─┬─  
//     │  ╰─── Expected top-level declaration, found Let
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found Word
//     ╭─[ 0:11:5 ]
//     │
//  11 │ let misspelling = foo.names;
//     │     ─────┬─────  
//     │          ╰─────── Expected top-level declaration, found Word
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found Equals
//     ╭─[ 0:11:17 ]
//     │
//  11 │ let misspelling = foo.names;
//     │                 ┬  
//     │                 ╰── Expected top-level declaration, found Equals
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found Word
//     ╭─[ 0:11:19 ]
//     │
//  11 │ let misspelling = foo.names;
//     │                   ─┬─  
//     │                    ╰─── Expected top-level declaration, found Word
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found Dot
//     ╭─[ 0:11:22 ]
//     │
//  11 │ let misspelling = foo.names;
//     │                      ┬  
//     │                      ╰── Expected top-level declaration, found Dot
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found Word
//     ╭─[ 0:11:23 ]
//     │
//  11 │ let misspelling = foo.names;
//     │                       ──┬──  
//     │                         ╰──── Expected top-level declaration, found Word
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found Semicolon
//     ╭─[ 0:11:28 ]
//     │
//  11 │ let misspelling = foo.names;
//     │                            ┬  
//     │                            ╰── Expected top-level declaration, found Semicolon
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found Let
//     ╭─[ 0:12:1 ]
//     │
//  12 │ let nonsense = foo.name.field;
//     │ ─┬─  
//     │  ╰─── Expected top-level declaration, found Let
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found Word
//     ╭─[ 0:12:5 ]
//     │
//  12 │ let nonsense = foo.name.field;
//     │     ────┬───  
//     │         ╰───── Expected top-level declaration, found Word
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found Equals
//     ╭─[ 0:12:14 ]
//     │
//  12 │ let nonsense = foo.name.field;
//     │              ┬  
//     │              ╰── Expected top-level declaration, found Equals
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found Word
//     ╭─[ 0:12:16 ]
//     │
//  12 │ let nonsense = foo.name.field;
//     │                ─┬─  
//     │                 ╰─── Expected top-level declaration, found Word
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found Dot
//     ╭─[ 0:12:19 ]
//     │
//  12 │ let nonsense = foo.name.field;
//     │                   ┬  
//     │                   ╰── Expected top-level declaration, found Dot
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found Word
//     ╭─[ 0:12:20 ]
//     │
//  12 │ let nonsense = foo.name.field;
//     │                    ──┬─  
//     │                      ╰─── Expected top-level declaration, found Word
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found Dot
//     ╭─[ 0:12:24 ]
//     │
//  12 │ let nonsense = foo.name.field;
//     │                        ┬  
//     │                        ╰── Expected top-level declaration, found Dot
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found Word
//     ╭─[ 0:12:25 ]
//     │
//  12 │ let nonsense = foo.name.field;
//     │                         ──┬──  
//     │                           ╰──── Expected top-level declaration, found Word
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found Semicolon
//     ╭─[ 0:12:30 ]
//     │
//  12 │ let nonsense = foo.name.field;
//     │                              ┬  
//     │                              ╰── Expected top-level declaration, found Semicolon
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found Let
//     ╭─[ 0:14:1 ]
//     │
//  14 │ let array_access_1 = [1, 2, 3][0];
//     │ ─┬─  
//     │  ╰─── Expected top-level declaration, found Let
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found Word
//     ╭─[ 0:14:5 ]
//     │
//  14 │ let array_access_1 = [1, 2, 3][0];
//     │     ───────┬──────  
//     │            ╰──────── Expected top-level declaration, found Word
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found Equals
//     ╭─[ 0:14:20 ]
//     │
//  14 │ let array_access_1 = [1, 2, 3][0];
//     │                    ┬  
//     │                    ╰── Expected top-level declaration, found Equals
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found LBracket
//     ╭─[ 0:14:22 ]
//     │
//  14 │ let array_access_1 = [1, 2, 3][0];
//     │                      ┬  
//     │                      ╰── Expected top-level declaration, found LBracket
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found IntegerLiteral
//     ╭─[ 0:14:23 ]
//     │
//  14 │ let array_access_1 = [1, 2, 3][0];
//     │                       ┬  
//     │                       ╰── Expected top-level declaration, found IntegerLiteral
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found Comma
//     ╭─[ 0:14:24 ]
//     │
//  14 │ let array_access_1 = [1, 2, 3][0];
//     │                        ┬  
//     │                        ╰── Expected top-level declaration, found Comma
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found IntegerLiteral
//     ╭─[ 0:14:26 ]
//     │
//  14 │ let array_access_1 = [1, 2, 3][0];
//     │                          ┬  
//     │                          ╰── Expected top-level declaration, found IntegerLiteral
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found Comma
//     ╭─[ 0:14:27 ]
//     │
//  14 │ let array_access_1 = [1, 2, 3][0];
//     │                           ┬  
//     │                           ╰── Expected top-level declaration, found Comma
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found IntegerLiteral
//     ╭─[ 0:14:29 ]
//     │
//  14 │ let array_access_1 = [1, 2, 3][0];
//     │                             ┬  
//     │                             ╰── Expected top-level declaration, found IntegerLiteral
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found RBracket
//     ╭─[ 0:14:30 ]
//     │
//  14 │ let array_access_1 = [1, 2, 3][0];
//     │                              ┬  
//     │                              ╰── Expected top-level declaration, found RBracket
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found LBracket
//     ╭─[ 0:14:31 ]
//     │
//  14 │ let array_access_1 = [1, 2, 3][0];
//     │                               ┬  
//     │                               ╰── Expected top-level declaration, found LBracket
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found IntegerLiteral
//     ╭─[ 0:14:32 ]
//     │
//  14 │ let array_access_1 = [1, 2, 3][0];
//     │                                ┬  
//     │                                ╰── Expected top-level declaration, found IntegerLiteral
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found RBracket
//     ╭─[ 0:14:33 ]
//     │
//  14 │ let array_access_1 = [1, 2, 3][0];
//     │                                 ┬  
//     │                                 ╰── Expected top-level declaration, found RBracket
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found Semicolon
//     ╭─[ 0:14:34 ]
//     │
//  14 │ let array_access_1 = [1, 2, 3][0];
//     │                                  ┬  
//     │                                  ╰── Expected top-level declaration, found Semicolon
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found Let
//     ╭─[ 0:15:1 ]
//     │
//  15 │ let array_access_2 = [1,2, 3]["hi"];
//     │ ─┬─  
//     │  ╰─── Expected top-level declaration, found Let
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found Word
//     ╭─[ 0:15:5 ]
//     │
//  15 │ let array_access_2 = [1,2, 3]["hi"];
//     │     ───────┬──────  
//     │            ╰──────── Expected top-level declaration, found Word
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found Equals
//     ╭─[ 0:15:20 ]
//     │
//  15 │ let array_access_2 = [1,2, 3]["hi"];
//     │                    ┬  
//     │                    ╰── Expected top-level declaration, found Equals
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found LBracket
//     ╭─[ 0:15:22 ]
//     │
//  15 │ let array_access_2 = [1,2, 3]["hi"];
//     │                      ┬  
//     │                      ╰── Expected top-level declaration, found LBracket
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found IntegerLiteral
//     ╭─[ 0:15:23 ]
//     │
//  15 │ let array_access_2 = [1,2, 3]["hi"];
//     │                       ┬  
//     │                       ╰── Expected top-level declaration, found IntegerLiteral
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found Comma
//     ╭─[ 0:15:24 ]
//     │
//  15 │ let array_access_2 = [1,2, 3]["hi"];
//     │                        ┬  
//     │                        ╰── Expected top-level declaration, found Comma
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found IntegerLiteral
//     ╭─[ 0:15:25 ]
//     │
//  15 │ let array_access_2 = [1,2, 3]["hi"];
//     │                         ┬  
//     │                         ╰── Expected top-level declaration, found IntegerLiteral
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found Comma
//     ╭─[ 0:15:26 ]
//     │
//  15 │ let array_access_2 = [1,2, 3]["hi"];
//     │                          ┬  
//     │                          ╰── Expected top-level declaration, found Comma
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found IntegerLiteral
//     ╭─[ 0:15:28 ]
//     │
//  15 │ let array_access_2 = [1,2, 3]["hi"];
//     │                            ┬  
//     │                            ╰── Expected top-level declaration, found IntegerLiteral
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found RBracket
//     ╭─[ 0:15:29 ]
//     │
//  15 │ let array_access_2 = [1,2, 3]["hi"];
//     │                             ┬  
//     │                             ╰── Expected top-level declaration, found RBracket
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found LBracket
//     ╭─[ 0:15:30 ]
//     │
//  15 │ let array_access_2 = [1,2, 3]["hi"];
//     │                              ┬  
//     │                              ╰── Expected top-level declaration, found LBracket
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found Quote
//     ╭─[ 0:15:31 ]
//     │
//  15 │ let array_access_2 = [1,2, 3]["hi"];
//     │                               ┬  
//     │                               ╰── Expected top-level declaration, found Quote
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found Word
//     ╭─[ 0:15:32 ]
//     │
//  15 │ let array_access_2 = [1,2, 3]["hi"];
//     │                                ─┬  
//     │                                 ╰── Expected top-level declaration, found Word
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found Quote
//     ╭─[ 0:15:34 ]
//     │
//  15 │ let array_access_2 = [1,2, 3]["hi"];
//     │                                  ┬  
//     │                                  ╰── Expected top-level declaration, found Quote
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found RBracket
//     ╭─[ 0:15:35 ]
//     │
//  15 │ let array_access_2 = [1,2, 3]["hi"];
//     │                                   ┬  
//     │                                   ╰── Expected top-level declaration, found RBracket
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found Semicolon
//     ╭─[ 0:15:36 ]
//     │
//  15 │ let array_access_2 = [1,2, 3]["hi"];
//     │                                    ┬  
//     │                                    ╰── Expected top-level declaration, found Semicolon
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found Let
//     ╭─[ 0:16:1 ]
//     │
//  16 │ let array_access_3 = [1,2, 3][foo.age];
//     │ ─┬─  
//     │  ╰─── Expected top-level declaration, found Let
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found Word
//     ╭─[ 0:16:5 ]
//     │
//  16 │ let array_access_3 = [1,2, 3][foo.age];
//     │     ───────┬──────  
//     │            ╰──────── Expected top-level declaration, found Word
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found Equals
//     ╭─[ 0:16:20 ]
//     │
//  16 │ let array_access_3 = [1,2, 3][foo.age];
//     │                    ┬  
//     │                    ╰── Expected top-level declaration, found Equals
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found LBracket
//     ╭─[ 0:16:22 ]
//     │
//  16 │ let array_access_3 = [1,2, 3][foo.age];
//     │                      ┬  
//     │                      ╰── Expected top-level declaration, found LBracket
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found IntegerLiteral
//     ╭─[ 0:16:23 ]
//     │
//  16 │ let array_access_3 = [1,2, 3][foo.age];
//     │                       ┬  
//     │                       ╰── Expected top-level declaration, found IntegerLiteral
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found Comma
//     ╭─[ 0:16:24 ]
//     │
//  16 │ let array_access_3 = [1,2, 3][foo.age];
//     │                        ┬  
//     │                        ╰── Expected top-level declaration, found Comma
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found IntegerLiteral
//     ╭─[ 0:16:25 ]
//     │
//  16 │ let array_access_3 = [1,2, 3][foo.age];
//     │                         ┬  
//     │                         ╰── Expected top-level declaration, found IntegerLiteral
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found Comma
//     ╭─[ 0:16:26 ]
//     │
//  16 │ let array_access_3 = [1,2, 3][foo.age];
//     │                          ┬  
//     │                          ╰── Expected top-level declaration, found Comma
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found IntegerLiteral
//     ╭─[ 0:16:28 ]
//     │
//  16 │ let array_access_3 = [1,2, 3][foo.age];
//     │                            ┬  
//     │                            ╰── Expected top-level declaration, found IntegerLiteral
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found RBracket
//     ╭─[ 0:16:29 ]
//     │
//  16 │ let array_access_3 = [1,2, 3][foo.age];
//     │                             ┬  
//     │                             ╰── Expected top-level declaration, found RBracket
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found LBracket
//     ╭─[ 0:16:30 ]
//     │
//  16 │ let array_access_3 = [1,2, 3][foo.age];
//     │                              ┬  
//     │                              ╰── Expected top-level declaration, found LBracket
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found Word
//     ╭─[ 0:16:31 ]
//     │
//  16 │ let array_access_3 = [1,2, 3][foo.age];
//     │                               ─┬─  
//     │                                ╰─── Expected top-level declaration, found Word
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found Dot
//     ╭─[ 0:16:34 ]
//     │
//  16 │ let array_access_3 = [1,2, 3][foo.age];
//     │                                  ┬  
//     │                                  ╰── Expected top-level declaration, found Dot
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found Word
//     ╭─[ 0:16:35 ]
//     │
//  16 │ let array_access_3 = [1,2, 3][foo.age];
//     │                                   ─┬─  
//     │                                    ╰─── Expected top-level declaration, found Word
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found RBracket
//     ╭─[ 0:16:38 ]
//     │
//  16 │ let array_access_3 = [1,2, 3][foo.age];
//     │                                      ┬  
//     │                                      ╰── Expected top-level declaration, found RBracket
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found Semicolon
//     ╭─[ 0:16:39 ]
//     │
//  16 │ let array_access_3 = [1,2, 3][foo.age];
//     │                                       ┬  
//     │                                       ╰── Expected top-level declaration, found Semicolon
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found Let
//     ╭─[ 0:17:1 ]
//     │
//  17 │ let array_access_4 = [1,2, 3][foo.name];
//     │ ─┬─  
//     │  ╰─── Expected top-level declaration, found Let
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found Word
//     ╭─[ 0:17:5 ]
//     │
//  17 │ let array_access_4 = [1,2, 3][foo.name];
//     │     ───────┬──────  
//     │            ╰──────── Expected top-level declaration, found Word
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found Equals
//     ╭─[ 0:17:20 ]
//     │
//  17 │ let array_access_4 = [1,2, 3][foo.name];
//     │                    ┬  
//     │                    ╰── Expected top-level declaration, found Equals
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found LBracket
//     ╭─[ 0:17:22 ]
//     │
//  17 │ let array_access_4 = [1,2, 3][foo.name];
//     │                      ┬  
//     │                      ╰── Expected top-level declaration, found LBracket
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found IntegerLiteral
//     ╭─[ 0:17:23 ]
//     │
//  17 │ let array_access_4 = [1,2, 3][foo.name];
//     │                       ┬  
//     │                       ╰── Expected top-level declaration, found IntegerLiteral
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found Comma
//     ╭─[ 0:17:24 ]
//     │
//  17 │ let array_access_4 = [1,2, 3][foo.name];
//     │                        ┬  
//     │                        ╰── Expected top-level declaration, found Comma
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found IntegerLiteral
//     ╭─[ 0:17:25 ]
//     │
//  17 │ let array_access_4 = [1,2, 3][foo.name];
//     │                         ┬  
//     │                         ╰── Expected top-level declaration, found IntegerLiteral
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found Comma
//     ╭─[ 0:17:26 ]
//     │
//  17 │ let array_access_4 = [1,2, 3][foo.name];
//     │                          ┬  
//     │                          ╰── Expected top-level declaration, found Comma
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found IntegerLiteral
//     ╭─[ 0:17:28 ]
//     │
//  17 │ let array_access_4 = [1,2, 3][foo.name];
//     │                            ┬  
//     │                            ╰── Expected top-level declaration, found IntegerLiteral
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found RBracket
//     ╭─[ 0:17:29 ]
//     │
//  17 │ let array_access_4 = [1,2, 3][foo.name];
//     │                             ┬  
//     │                             ╰── Expected top-level declaration, found RBracket
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found LBracket
//     ╭─[ 0:17:30 ]
//     │
//  17 │ let array_access_4 = [1,2, 3][foo.name];
//     │                              ┬  
//     │                              ╰── Expected top-level declaration, found LBracket
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found Word
//     ╭─[ 0:17:31 ]
//     │
//  17 │ let array_access_4 = [1,2, 3][foo.name];
//     │                               ─┬─  
//     │                                ╰─── Expected top-level declaration, found Word
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found Dot
//     ╭─[ 0:17:34 ]
//     │
//  17 │ let array_access_4 = [1,2, 3][foo.name];
//     │                                  ┬  
//     │                                  ╰── Expected top-level declaration, found Dot
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found Word
//     ╭─[ 0:17:35 ]
//     │
//  17 │ let array_access_4 = [1,2, 3][foo.name];
//     │                                   ──┬─  
//     │                                     ╰─── Expected top-level declaration, found Word
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found RBracket
//     ╭─[ 0:17:39 ]
//     │
//  17 │ let array_access_4 = [1,2, 3][foo.name];
//     │                                       ┬  
//     │                                       ╰── Expected top-level declaration, found RBracket
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found Semicolon
//     ╭─[ 0:17:40 ]
//     │
//  17 │ let array_access_4 = [1,2, 3][foo.name];
//     │                                        ┬  
//     │                                        ╰── Expected top-level declaration, found Semicolon
//     │ 
//     │ Note: Error code: E0010
// ────╯
