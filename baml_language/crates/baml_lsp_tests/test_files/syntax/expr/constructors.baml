class MyClass {
    a int
    b string
}

let x = MyClass { a: 1, b: "2" };

let y = MyClass { a: 1, ...x };


let default_person = Person {
    name: "John Doe",
    age: 20,
    poem: "Never was there a man more plain."
};


class Person {
  name string
  age int
  poem string
}

//----
//- diagnostics
// Error: Expected top-level declaration, found Let
//    ╭─[ 0:6:1 ]
//    │
//  6 │ let x = MyClass { a: 1, b: "2" };
//    │ ─┬─  
//    │  ╰─── Expected top-level declaration, found Let
//    │ 
//    │ Note: Error code: E0010
// ───╯
// Error: Expected top-level declaration, found Word
//    ╭─[ 0:6:5 ]
//    │
//  6 │ let x = MyClass { a: 1, b: "2" };
//    │     ┬  
//    │     ╰── Expected top-level declaration, found Word
//    │ 
//    │ Note: Error code: E0010
// ───╯
// Error: Expected top-level declaration, found Equals
//    ╭─[ 0:6:7 ]
//    │
//  6 │ let x = MyClass { a: 1, b: "2" };
//    │       ┬  
//    │       ╰── Expected top-level declaration, found Equals
//    │ 
//    │ Note: Error code: E0010
// ───╯
// Error: Expected top-level declaration, found Word
//    ╭─[ 0:6:9 ]
//    │
//  6 │ let x = MyClass { a: 1, b: "2" };
//    │         ───┬───  
//    │            ╰───── Expected top-level declaration, found Word
//    │ 
//    │ Note: Error code: E0010
// ───╯
// Error: Expected top-level declaration, found LBrace
//    ╭─[ 0:6:17 ]
//    │
//  6 │ let x = MyClass { a: 1, b: "2" };
//    │                 ┬  
//    │                 ╰── Expected top-level declaration, found LBrace
//    │ 
//    │ Note: Error code: E0010
// ───╯
// Error: Expected top-level declaration, found Word
//    ╭─[ 0:6:19 ]
//    │
//  6 │ let x = MyClass { a: 1, b: "2" };
//    │                   ┬  
//    │                   ╰── Expected top-level declaration, found Word
//    │ 
//    │ Note: Error code: E0010
// ───╯
// Error: Expected top-level declaration, found Colon
//    ╭─[ 0:6:20 ]
//    │
//  6 │ let x = MyClass { a: 1, b: "2" };
//    │                    ┬  
//    │                    ╰── Expected top-level declaration, found Colon
//    │ 
//    │ Note: Error code: E0010
// ───╯
// Error: Expected top-level declaration, found IntegerLiteral
//    ╭─[ 0:6:22 ]
//    │
//  6 │ let x = MyClass { a: 1, b: "2" };
//    │                      ┬  
//    │                      ╰── Expected top-level declaration, found IntegerLiteral
//    │ 
//    │ Note: Error code: E0010
// ───╯
// Error: Expected top-level declaration, found Comma
//    ╭─[ 0:6:23 ]
//    │
//  6 │ let x = MyClass { a: 1, b: "2" };
//    │                       ┬  
//    │                       ╰── Expected top-level declaration, found Comma
//    │ 
//    │ Note: Error code: E0010
// ───╯
// Error: Expected top-level declaration, found Word
//    ╭─[ 0:6:25 ]
//    │
//  6 │ let x = MyClass { a: 1, b: "2" };
//    │                         ┬  
//    │                         ╰── Expected top-level declaration, found Word
//    │ 
//    │ Note: Error code: E0010
// ───╯
// Error: Expected top-level declaration, found Colon
//    ╭─[ 0:6:26 ]
//    │
//  6 │ let x = MyClass { a: 1, b: "2" };
//    │                          ┬  
//    │                          ╰── Expected top-level declaration, found Colon
//    │ 
//    │ Note: Error code: E0010
// ───╯
// Error: Expected top-level declaration, found Quote
//    ╭─[ 0:6:28 ]
//    │
//  6 │ let x = MyClass { a: 1, b: "2" };
//    │                            ┬  
//    │                            ╰── Expected top-level declaration, found Quote
//    │ 
//    │ Note: Error code: E0010
// ───╯
// Error: Expected top-level declaration, found IntegerLiteral
//    ╭─[ 0:6:29 ]
//    │
//  6 │ let x = MyClass { a: 1, b: "2" };
//    │                             ┬  
//    │                             ╰── Expected top-level declaration, found IntegerLiteral
//    │ 
//    │ Note: Error code: E0010
// ───╯
// Error: Expected top-level declaration, found Quote
//    ╭─[ 0:6:30 ]
//    │
//  6 │ let x = MyClass { a: 1, b: "2" };
//    │                              ┬  
//    │                              ╰── Expected top-level declaration, found Quote
//    │ 
//    │ Note: Error code: E0010
// ───╯
// Error: Expected top-level declaration, found RBrace
//    ╭─[ 0:6:32 ]
//    │
//  6 │ let x = MyClass { a: 1, b: "2" };
//    │                                ┬  
//    │                                ╰── Expected top-level declaration, found RBrace
//    │ 
//    │ Note: Error code: E0010
// ───╯
// Error: Expected top-level declaration, found Semicolon
//    ╭─[ 0:6:33 ]
//    │
//  6 │ let x = MyClass { a: 1, b: "2" };
//    │                                 ┬  
//    │                                 ╰── Expected top-level declaration, found Semicolon
//    │ 
//    │ Note: Error code: E0010
// ───╯
// Error: Expected top-level declaration, found Let
//    ╭─[ 0:8:1 ]
//    │
//  8 │ let y = MyClass { a: 1, ...x };
//    │ ─┬─  
//    │  ╰─── Expected top-level declaration, found Let
//    │ 
//    │ Note: Error code: E0010
// ───╯
// Error: Expected top-level declaration, found Word
//    ╭─[ 0:8:5 ]
//    │
//  8 │ let y = MyClass { a: 1, ...x };
//    │     ┬  
//    │     ╰── Expected top-level declaration, found Word
//    │ 
//    │ Note: Error code: E0010
// ───╯
// Error: Expected top-level declaration, found Equals
//    ╭─[ 0:8:7 ]
//    │
//  8 │ let y = MyClass { a: 1, ...x };
//    │       ┬  
//    │       ╰── Expected top-level declaration, found Equals
//    │ 
//    │ Note: Error code: E0010
// ───╯
// Error: Expected top-level declaration, found Word
//    ╭─[ 0:8:9 ]
//    │
//  8 │ let y = MyClass { a: 1, ...x };
//    │         ───┬───  
//    │            ╰───── Expected top-level declaration, found Word
//    │ 
//    │ Note: Error code: E0010
// ───╯
// Error: Expected top-level declaration, found LBrace
//    ╭─[ 0:8:17 ]
//    │
//  8 │ let y = MyClass { a: 1, ...x };
//    │                 ┬  
//    │                 ╰── Expected top-level declaration, found LBrace
//    │ 
//    │ Note: Error code: E0010
// ───╯
// Error: Expected top-level declaration, found Word
//    ╭─[ 0:8:19 ]
//    │
//  8 │ let y = MyClass { a: 1, ...x };
//    │                   ┬  
//    │                   ╰── Expected top-level declaration, found Word
//    │ 
//    │ Note: Error code: E0010
// ───╯
// Error: Expected top-level declaration, found Colon
//    ╭─[ 0:8:20 ]
//    │
//  8 │ let y = MyClass { a: 1, ...x };
//    │                    ┬  
//    │                    ╰── Expected top-level declaration, found Colon
//    │ 
//    │ Note: Error code: E0010
// ───╯
// Error: Expected top-level declaration, found IntegerLiteral
//    ╭─[ 0:8:22 ]
//    │
//  8 │ let y = MyClass { a: 1, ...x };
//    │                      ┬  
//    │                      ╰── Expected top-level declaration, found IntegerLiteral
//    │ 
//    │ Note: Error code: E0010
// ───╯
// Error: Expected top-level declaration, found Comma
//    ╭─[ 0:8:23 ]
//    │
//  8 │ let y = MyClass { a: 1, ...x };
//    │                       ┬  
//    │                       ╰── Expected top-level declaration, found Comma
//    │ 
//    │ Note: Error code: E0010
// ───╯
// Error: Expected top-level declaration, found Dot
//    ╭─[ 0:8:25 ]
//    │
//  8 │ let y = MyClass { a: 1, ...x };
//    │                         ┬  
//    │                         ╰── Expected top-level declaration, found Dot
//    │ 
//    │ Note: Error code: E0010
// ───╯
// Error: Expected top-level declaration, found Dot
//    ╭─[ 0:8:26 ]
//    │
//  8 │ let y = MyClass { a: 1, ...x };
//    │                          ┬  
//    │                          ╰── Expected top-level declaration, found Dot
//    │ 
//    │ Note: Error code: E0010
// ───╯
// Error: Expected top-level declaration, found Dot
//    ╭─[ 0:8:27 ]
//    │
//  8 │ let y = MyClass { a: 1, ...x };
//    │                           ┬  
//    │                           ╰── Expected top-level declaration, found Dot
//    │ 
//    │ Note: Error code: E0010
// ───╯
// Error: Expected top-level declaration, found Word
//    ╭─[ 0:8:28 ]
//    │
//  8 │ let y = MyClass { a: 1, ...x };
//    │                            ┬  
//    │                            ╰── Expected top-level declaration, found Word
//    │ 
//    │ Note: Error code: E0010
// ───╯
// Error: Expected top-level declaration, found RBrace
//    ╭─[ 0:8:30 ]
//    │
//  8 │ let y = MyClass { a: 1, ...x };
//    │                              ┬  
//    │                              ╰── Expected top-level declaration, found RBrace
//    │ 
//    │ Note: Error code: E0010
// ───╯
// Error: Expected top-level declaration, found Semicolon
//    ╭─[ 0:8:31 ]
//    │
//  8 │ let y = MyClass { a: 1, ...x };
//    │                               ┬  
//    │                               ╰── Expected top-level declaration, found Semicolon
//    │ 
//    │ Note: Error code: E0010
// ───╯
// Error: Expected top-level declaration, found Let
//     ╭─[ 0:11:1 ]
//     │
//  11 │ let default_person = Person {
//     │ ─┬─  
//     │  ╰─── Expected top-level declaration, found Let
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found Word
//     ╭─[ 0:11:5 ]
//     │
//  11 │ let default_person = Person {
//     │     ───────┬──────  
//     │            ╰──────── Expected top-level declaration, found Word
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found Equals
//     ╭─[ 0:11:20 ]
//     │
//  11 │ let default_person = Person {
//     │                    ┬  
//     │                    ╰── Expected top-level declaration, found Equals
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found Word
//     ╭─[ 0:11:22 ]
//     │
//  11 │ let default_person = Person {
//     │                      ───┬──  
//     │                         ╰──── Expected top-level declaration, found Word
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found LBrace
//     ╭─[ 0:11:29 ]
//     │
//  11 │ let default_person = Person {
//     │                             ┬  
//     │                             ╰── Expected top-level declaration, found LBrace
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found Word
//     ╭─[ 0:12:5 ]
//     │
//  12 │     name: "John Doe",
//     │     ──┬─  
//     │       ╰─── Expected top-level declaration, found Word
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found Colon
//     ╭─[ 0:12:9 ]
//     │
//  12 │     name: "John Doe",
//     │         ┬  
//     │         ╰── Expected top-level declaration, found Colon
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found Quote
//     ╭─[ 0:12:11 ]
//     │
//  12 │     name: "John Doe",
//     │           ┬  
//     │           ╰── Expected top-level declaration, found Quote
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found Word
//     ╭─[ 0:12:12 ]
//     │
//  12 │     name: "John Doe",
//     │            ──┬─  
//     │              ╰─── Expected top-level declaration, found Word
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found Word
//     ╭─[ 0:12:17 ]
//     │
//  12 │     name: "John Doe",
//     │                 ─┬─  
//     │                  ╰─── Expected top-level declaration, found Word
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found Quote
//     ╭─[ 0:12:20 ]
//     │
//  12 │     name: "John Doe",
//     │                    ┬  
//     │                    ╰── Expected top-level declaration, found Quote
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found Comma
//     ╭─[ 0:12:21 ]
//     │
//  12 │     name: "John Doe",
//     │                     ┬  
//     │                     ╰── Expected top-level declaration, found Comma
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found Word
//     ╭─[ 0:13:5 ]
//     │
//  13 │     age: 20,
//     │     ─┬─  
//     │      ╰─── Expected top-level declaration, found Word
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found Colon
//     ╭─[ 0:13:8 ]
//     │
//  13 │     age: 20,
//     │        ┬  
//     │        ╰── Expected top-level declaration, found Colon
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found IntegerLiteral
//     ╭─[ 0:13:10 ]
//     │
//  13 │     age: 20,
//     │          ─┬  
//     │           ╰── Expected top-level declaration, found IntegerLiteral
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found Comma
//     ╭─[ 0:13:12 ]
//     │
//  13 │     age: 20,
//     │            ┬  
//     │            ╰── Expected top-level declaration, found Comma
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found Word
//     ╭─[ 0:14:5 ]
//     │
//  14 │     poem: "Never was there a man more plain."
//     │     ──┬─  
//     │       ╰─── Expected top-level declaration, found Word
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found Colon
//     ╭─[ 0:14:9 ]
//     │
//  14 │     poem: "Never was there a man more plain."
//     │         ┬  
//     │         ╰── Expected top-level declaration, found Colon
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found Quote
//     ╭─[ 0:14:11 ]
//     │
//  14 │     poem: "Never was there a man more plain."
//     │           ┬  
//     │           ╰── Expected top-level declaration, found Quote
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found Word
//     ╭─[ 0:14:12 ]
//     │
//  14 │     poem: "Never was there a man more plain."
//     │            ──┬──  
//     │              ╰──── Expected top-level declaration, found Word
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found Word
//     ╭─[ 0:14:18 ]
//     │
//  14 │     poem: "Never was there a man more plain."
//     │                  ─┬─  
//     │                   ╰─── Expected top-level declaration, found Word
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found Word
//     ╭─[ 0:14:22 ]
//     │
//  14 │     poem: "Never was there a man more plain."
//     │                      ──┬──  
//     │                        ╰──── Expected top-level declaration, found Word
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found Word
//     ╭─[ 0:14:28 ]
//     │
//  14 │     poem: "Never was there a man more plain."
//     │                            ┬  
//     │                            ╰── Expected top-level declaration, found Word
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found Word
//     ╭─[ 0:14:30 ]
//     │
//  14 │     poem: "Never was there a man more plain."
//     │                              ─┬─  
//     │                               ╰─── Expected top-level declaration, found Word
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found Word
//     ╭─[ 0:14:34 ]
//     │
//  14 │     poem: "Never was there a man more plain."
//     │                                  ──┬─  
//     │                                    ╰─── Expected top-level declaration, found Word
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found Word
//     ╭─[ 0:14:39 ]
//     │
//  14 │     poem: "Never was there a man more plain."
//     │                                       ──┬──  
//     │                                         ╰──── Expected top-level declaration, found Word
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found Dot
//     ╭─[ 0:14:44 ]
//     │
//  14 │     poem: "Never was there a man more plain."
//     │                                            ┬  
//     │                                            ╰── Expected top-level declaration, found Dot
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found Quote
//     ╭─[ 0:14:45 ]
//     │
//  14 │     poem: "Never was there a man more plain."
//     │                                             ┬  
//     │                                             ╰── Expected top-level declaration, found Quote
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found RBrace
//     ╭─[ 0:15:1 ]
//     │
//  15 │ };
//     │ ┬  
//     │ ╰── Expected top-level declaration, found RBrace
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found Semicolon
//     ╭─[ 0:15:2 ]
//     │
//  15 │ };
//     │  ┬  
//     │  ╰── Expected top-level declaration, found Semicolon
//     │ 
//     │ Note: Error code: E0010
// ────╯
