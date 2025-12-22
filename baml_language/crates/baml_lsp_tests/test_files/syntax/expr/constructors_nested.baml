class Foo {
  bar Bar
}

class Bar {
  name string
}

let x = Foo { bar: Bar { name: 10 }};

// error: Error validating: Bar.name expected type string, but found int
//   -->  expr/constructors_nested.baml:9
//    | 
//  8 | 
//  9 | let x = Foo { bar: Bar { name: 10 }};
//    |

//----
//- diagnostics
// Error: Expected top-level declaration, found Let
//    ╭─[ 0:9:1 ]
//    │
//  9 │ let x = Foo { bar: Bar { name: 10 }};
//    │ ─┬─  
//    │  ╰─── Expected top-level declaration, found Let
//    │ 
//    │ Note: Error code: E0010
// ───╯
// Error: Expected top-level declaration, found Word
//    ╭─[ 0:9:5 ]
//    │
//  9 │ let x = Foo { bar: Bar { name: 10 }};
//    │     ┬  
//    │     ╰── Expected top-level declaration, found Word
//    │ 
//    │ Note: Error code: E0010
// ───╯
// Error: Expected top-level declaration, found Equals
//    ╭─[ 0:9:7 ]
//    │
//  9 │ let x = Foo { bar: Bar { name: 10 }};
//    │       ┬  
//    │       ╰── Expected top-level declaration, found Equals
//    │ 
//    │ Note: Error code: E0010
// ───╯
// Error: Expected top-level declaration, found Word
//    ╭─[ 0:9:9 ]
//    │
//  9 │ let x = Foo { bar: Bar { name: 10 }};
//    │         ─┬─  
//    │          ╰─── Expected top-level declaration, found Word
//    │ 
//    │ Note: Error code: E0010
// ───╯
// Error: Expected top-level declaration, found LBrace
//    ╭─[ 0:9:13 ]
//    │
//  9 │ let x = Foo { bar: Bar { name: 10 }};
//    │             ┬  
//    │             ╰── Expected top-level declaration, found LBrace
//    │ 
//    │ Note: Error code: E0010
// ───╯
// Error: Expected top-level declaration, found Word
//    ╭─[ 0:9:15 ]
//    │
//  9 │ let x = Foo { bar: Bar { name: 10 }};
//    │               ─┬─  
//    │                ╰─── Expected top-level declaration, found Word
//    │ 
//    │ Note: Error code: E0010
// ───╯
// Error: Expected top-level declaration, found Colon
//    ╭─[ 0:9:18 ]
//    │
//  9 │ let x = Foo { bar: Bar { name: 10 }};
//    │                  ┬  
//    │                  ╰── Expected top-level declaration, found Colon
//    │ 
//    │ Note: Error code: E0010
// ───╯
// Error: Expected top-level declaration, found Word
//    ╭─[ 0:9:20 ]
//    │
//  9 │ let x = Foo { bar: Bar { name: 10 }};
//    │                    ─┬─  
//    │                     ╰─── Expected top-level declaration, found Word
//    │ 
//    │ Note: Error code: E0010
// ───╯
// Error: Expected top-level declaration, found LBrace
//    ╭─[ 0:9:24 ]
//    │
//  9 │ let x = Foo { bar: Bar { name: 10 }};
//    │                        ┬  
//    │                        ╰── Expected top-level declaration, found LBrace
//    │ 
//    │ Note: Error code: E0010
// ───╯
// Error: Expected top-level declaration, found Word
//    ╭─[ 0:9:26 ]
//    │
//  9 │ let x = Foo { bar: Bar { name: 10 }};
//    │                          ──┬─  
//    │                            ╰─── Expected top-level declaration, found Word
//    │ 
//    │ Note: Error code: E0010
// ───╯
// Error: Expected top-level declaration, found Colon
//    ╭─[ 0:9:30 ]
//    │
//  9 │ let x = Foo { bar: Bar { name: 10 }};
//    │                              ┬  
//    │                              ╰── Expected top-level declaration, found Colon
//    │ 
//    │ Note: Error code: E0010
// ───╯
// Error: Expected top-level declaration, found IntegerLiteral
//    ╭─[ 0:9:32 ]
//    │
//  9 │ let x = Foo { bar: Bar { name: 10 }};
//    │                                ─┬  
//    │                                 ╰── Expected top-level declaration, found IntegerLiteral
//    │ 
//    │ Note: Error code: E0010
// ───╯
// Error: Expected top-level declaration, found RBrace
//    ╭─[ 0:9:35 ]
//    │
//  9 │ let x = Foo { bar: Bar { name: 10 }};
//    │                                   ┬  
//    │                                   ╰── Expected top-level declaration, found RBrace
//    │ 
//    │ Note: Error code: E0010
// ───╯
// Error: Expected top-level declaration, found RBrace
//    ╭─[ 0:9:36 ]
//    │
//  9 │ let x = Foo { bar: Bar { name: 10 }};
//    │                                    ┬  
//    │                                    ╰── Expected top-level declaration, found RBrace
//    │ 
//    │ Note: Error code: E0010
// ───╯
// Error: Expected top-level declaration, found Semicolon
//    ╭─[ 0:9:37 ]
//    │
//  9 │ let x = Foo { bar: Bar { name: 10 }};
//    │                                     ┬  
//    │                                     ╰── Expected top-level declaration, found Semicolon
//    │ 
//    │ Note: Error code: E0010
// ───╯
