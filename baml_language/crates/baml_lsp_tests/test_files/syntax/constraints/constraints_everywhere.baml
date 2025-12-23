client<llm> Bar {
  provider baml-openai-chat
}

class Foo {
  age int @assert(old_enough, {{this > 10}})
}

function FooToInt(foo: Foo, a: Foo @assert(really_old, {{this.age > 20}})) -> int @check(small_int, {{ this < 10 }}) {
   client Bar
   prompt #"fa"#
}

//----
//- diagnostics
// Error: Expected RParen, found At
//    ╭─[ 0:9:36 ]
//    │
//  9 │ function FooToInt(foo: Foo, a: Foo @assert(really_old, {{this.age > 20}})) -> int @check(small_int, {{ this < 10 }}) {
//    │                                    ┬  
//    │                                    ╰── Expected RParen, found At
//    │ 
//    │ Note: Error code: E0010
// ───╯
// Error: Expected return type (->), found At
//    ╭─[ 0:9:36 ]
//    │
//  9 │ function FooToInt(foo: Foo, a: Foo @assert(really_old, {{this.age > 20}})) -> int @check(small_int, {{ this < 10 }}) {
//    │                                    ┬  
//    │                                    ╰── Expected return type (->), found At
//    │ 
//    │ Note: Error code: E0010
// ───╯
// Error: Expected function body, found At
//    ╭─[ 0:9:36 ]
//    │
//  9 │ function FooToInt(foo: Foo, a: Foo @assert(really_old, {{this.age > 20}})) -> int @check(small_int, {{ this < 10 }}) {
//    │                                    ┬  
//    │                                    ╰── Expected function body, found At
//    │ 
//    │ Note: Error code: E0010
// ───╯
// Error: Expected top-level declaration, found At
//    ╭─[ 0:9:36 ]
//    │
//  9 │ function FooToInt(foo: Foo, a: Foo @assert(really_old, {{this.age > 20}})) -> int @check(small_int, {{ this < 10 }}) {
//    │                                    ┬  
//    │                                    ╰── Expected top-level declaration, found At
//    │ 
//    │ Note: Error code: E0010
// ───╯
// Error: Expected top-level declaration, found Word
//    ╭─[ 0:9:37 ]
//    │
//  9 │ function FooToInt(foo: Foo, a: Foo @assert(really_old, {{this.age > 20}})) -> int @check(small_int, {{ this < 10 }}) {
//    │                                     ───┬──  
//    │                                        ╰──── Expected top-level declaration, found Word
//    │ 
//    │ Note: Error code: E0010
// ───╯
// Error: Expected top-level declaration, found LParen
//    ╭─[ 0:9:43 ]
//    │
//  9 │ function FooToInt(foo: Foo, a: Foo @assert(really_old, {{this.age > 20}})) -> int @check(small_int, {{ this < 10 }}) {
//    │                                           ┬  
//    │                                           ╰── Expected top-level declaration, found LParen
//    │ 
//    │ Note: Error code: E0010
// ───╯
// Error: Expected top-level declaration, found Word
//    ╭─[ 0:9:44 ]
//    │
//  9 │ function FooToInt(foo: Foo, a: Foo @assert(really_old, {{this.age > 20}})) -> int @check(small_int, {{ this < 10 }}) {
//    │                                            ─────┬────  
//    │                                                 ╰────── Expected top-level declaration, found Word
//    │ 
//    │ Note: Error code: E0010
// ───╯
// Error: Expected top-level declaration, found Comma
//    ╭─[ 0:9:54 ]
//    │
//  9 │ function FooToInt(foo: Foo, a: Foo @assert(really_old, {{this.age > 20}})) -> int @check(small_int, {{ this < 10 }}) {
//    │                                                      ┬  
//    │                                                      ╰── Expected top-level declaration, found Comma
//    │ 
//    │ Note: Error code: E0010
// ───╯
// Error: Expected top-level declaration, found LBrace
//    ╭─[ 0:9:56 ]
//    │
//  9 │ function FooToInt(foo: Foo, a: Foo @assert(really_old, {{this.age > 20}})) -> int @check(small_int, {{ this < 10 }}) {
//    │                                                        ┬  
//    │                                                        ╰── Expected top-level declaration, found LBrace
//    │ 
//    │ Note: Error code: E0010
// ───╯
// Error: Expected top-level declaration, found LBrace
//    ╭─[ 0:9:57 ]
//    │
//  9 │ function FooToInt(foo: Foo, a: Foo @assert(really_old, {{this.age > 20}})) -> int @check(small_int, {{ this < 10 }}) {
//    │                                                         ┬  
//    │                                                         ╰── Expected top-level declaration, found LBrace
//    │ 
//    │ Note: Error code: E0010
// ───╯
// Error: Expected top-level declaration, found Word
//    ╭─[ 0:9:58 ]
//    │
//  9 │ function FooToInt(foo: Foo, a: Foo @assert(really_old, {{this.age > 20}})) -> int @check(small_int, {{ this < 10 }}) {
//    │                                                          ──┬─  
//    │                                                            ╰─── Expected top-level declaration, found Word
//    │ 
//    │ Note: Error code: E0010
// ───╯
// Error: Expected top-level declaration, found Dot
//    ╭─[ 0:9:62 ]
//    │
//  9 │ function FooToInt(foo: Foo, a: Foo @assert(really_old, {{this.age > 20}})) -> int @check(small_int, {{ this < 10 }}) {
//    │                                                              ┬  
//    │                                                              ╰── Expected top-level declaration, found Dot
//    │ 
//    │ Note: Error code: E0010
// ───╯
// Error: Expected top-level declaration, found Word
//    ╭─[ 0:9:63 ]
//    │
//  9 │ function FooToInt(foo: Foo, a: Foo @assert(really_old, {{this.age > 20}})) -> int @check(small_int, {{ this < 10 }}) {
//    │                                                               ─┬─  
//    │                                                                ╰─── Expected top-level declaration, found Word
//    │ 
//    │ Note: Error code: E0010
// ───╯
// Error: Expected top-level declaration, found Greater
//    ╭─[ 0:9:67 ]
//    │
//  9 │ function FooToInt(foo: Foo, a: Foo @assert(really_old, {{this.age > 20}})) -> int @check(small_int, {{ this < 10 }}) {
//    │                                                                   ┬  
//    │                                                                   ╰── Expected top-level declaration, found Greater
//    │ 
//    │ Note: Error code: E0010
// ───╯
// Error: Expected top-level declaration, found IntegerLiteral
//    ╭─[ 0:9:69 ]
//    │
//  9 │ function FooToInt(foo: Foo, a: Foo @assert(really_old, {{this.age > 20}})) -> int @check(small_int, {{ this < 10 }}) {
//    │                                                                     ─┬  
//    │                                                                      ╰── Expected top-level declaration, found IntegerLiteral
//    │ 
//    │ Note: Error code: E0010
// ───╯
// Error: Expected top-level declaration, found RBrace
//    ╭─[ 0:9:71 ]
//    │
//  9 │ function FooToInt(foo: Foo, a: Foo @assert(really_old, {{this.age > 20}})) -> int @check(small_int, {{ this < 10 }}) {
//    │                                                                       ┬  
//    │                                                                       ╰── Expected top-level declaration, found RBrace
//    │ 
//    │ Note: Error code: E0010
// ───╯
// Error: Expected top-level declaration, found RBrace
//    ╭─[ 0:9:72 ]
//    │
//  9 │ function FooToInt(foo: Foo, a: Foo @assert(really_old, {{this.age > 20}})) -> int @check(small_int, {{ this < 10 }}) {
//    │                                                                        ┬  
//    │                                                                        ╰── Expected top-level declaration, found RBrace
//    │ 
//    │ Note: Error code: E0010
// ───╯
// Error: Expected top-level declaration, found RParen
//    ╭─[ 0:9:73 ]
//    │
//  9 │ function FooToInt(foo: Foo, a: Foo @assert(really_old, {{this.age > 20}})) -> int @check(small_int, {{ this < 10 }}) {
//    │                                                                         ┬  
//    │                                                                         ╰── Expected top-level declaration, found RParen
//    │ 
//    │ Note: Error code: E0010
// ───╯
// Error: Expected top-level declaration, found RParen
//    ╭─[ 0:9:74 ]
//    │
//  9 │ function FooToInt(foo: Foo, a: Foo @assert(really_old, {{this.age > 20}})) -> int @check(small_int, {{ this < 10 }}) {
//    │                                                                          ┬  
//    │                                                                          ╰── Expected top-level declaration, found RParen
//    │ 
//    │ Note: Error code: E0010
// ───╯
// Error: Expected top-level declaration, found Arrow
//    ╭─[ 0:9:76 ]
//    │
//  9 │ function FooToInt(foo: Foo, a: Foo @assert(really_old, {{this.age > 20}})) -> int @check(small_int, {{ this < 10 }}) {
//    │                                                                            ─┬  
//    │                                                                             ╰── Expected top-level declaration, found Arrow
//    │ 
//    │ Note: Error code: E0010
// ───╯
// Error: Expected top-level declaration, found Word
//    ╭─[ 0:9:79 ]
//    │
//  9 │ function FooToInt(foo: Foo, a: Foo @assert(really_old, {{this.age > 20}})) -> int @check(small_int, {{ this < 10 }}) {
//    │                                                                               ─┬─  
//    │                                                                                ╰─── Expected top-level declaration, found Word
//    │ 
//    │ Note: Error code: E0010
// ───╯
// Error: Expected top-level declaration, found At
//    ╭─[ 0:9:83 ]
//    │
//  9 │ function FooToInt(foo: Foo, a: Foo @assert(really_old, {{this.age > 20}})) -> int @check(small_int, {{ this < 10 }}) {
//    │                                                                                   ┬  
//    │                                                                                   ╰── Expected top-level declaration, found At
//    │ 
//    │ Note: Error code: E0010
// ───╯
// Error: Expected top-level declaration, found Word
//    ╭─[ 0:9:84 ]
//    │
//  9 │ function FooToInt(foo: Foo, a: Foo @assert(really_old, {{this.age > 20}})) -> int @check(small_int, {{ this < 10 }}) {
//    │                                                                                    ──┬──  
//    │                                                                                      ╰──── Expected top-level declaration, found Word
//    │ 
//    │ Note: Error code: E0010
// ───╯
// Error: Expected top-level declaration, found LParen
//    ╭─[ 0:9:89 ]
//    │
//  9 │ function FooToInt(foo: Foo, a: Foo @assert(really_old, {{this.age > 20}})) -> int @check(small_int, {{ this < 10 }}) {
//    │                                                                                         ┬  
//    │                                                                                         ╰── Expected top-level declaration, found LParen
//    │ 
//    │ Note: Error code: E0010
// ───╯
// Error: Expected top-level declaration, found Word
//    ╭─[ 0:9:90 ]
//    │
//  9 │ function FooToInt(foo: Foo, a: Foo @assert(really_old, {{this.age > 20}})) -> int @check(small_int, {{ this < 10 }}) {
//    │                                                                                          ────┬────  
//    │                                                                                              ╰────── Expected top-level declaration, found Word
//    │ 
//    │ Note: Error code: E0010
// ───╯
// Error: Expected top-level declaration, found Comma
//    ╭─[ 0:9:99 ]
//    │
//  9 │ function FooToInt(foo: Foo, a: Foo @assert(really_old, {{this.age > 20}})) -> int @check(small_int, {{ this < 10 }}) {
//    │                                                                                                   ┬  
//    │                                                                                                   ╰── Expected top-level declaration, found Comma
//    │ 
//    │ Note: Error code: E0010
// ───╯
// Error: Expected top-level declaration, found LBrace
//    ╭─[ 0:9:101 ]
//    │
//  9 │ function FooToInt(foo: Foo, a: Foo @assert(really_old, {{this.age > 20}})) -> int @check(small_int, {{ this < 10 }}) {
//    │                                                                                                     ┬  
//    │                                                                                                     ╰── Expected top-level declaration, found LBrace
//    │ 
//    │ Note: Error code: E0010
// ───╯
// Error: Expected top-level declaration, found LBrace
//    ╭─[ 0:9:102 ]
//    │
//  9 │ function FooToInt(foo: Foo, a: Foo @assert(really_old, {{this.age > 20}})) -> int @check(small_int, {{ this < 10 }}) {
//    │                                                                                                      ┬  
//    │                                                                                                      ╰── Expected top-level declaration, found LBrace
//    │ 
//    │ Note: Error code: E0010
// ───╯
// Error: Expected top-level declaration, found Word
//    ╭─[ 0:9:104 ]
//    │
//  9 │ function FooToInt(foo: Foo, a: Foo @assert(really_old, {{this.age > 20}})) -> int @check(small_int, {{ this < 10 }}) {
//    │                                                                                                        ──┬─  
//    │                                                                                                          ╰─── Expected top-level declaration, found Word
//    │ 
//    │ Note: Error code: E0010
// ───╯
// Error: Expected top-level declaration, found Less
//    ╭─[ 0:9:109 ]
//    │
//  9 │ function FooToInt(foo: Foo, a: Foo @assert(really_old, {{this.age > 20}})) -> int @check(small_int, {{ this < 10 }}) {
//    │                                                                                                             ┬  
//    │                                                                                                             ╰── Expected top-level declaration, found Less
//    │ 
//    │ Note: Error code: E0010
// ───╯
// Error: Expected top-level declaration, found IntegerLiteral
//    ╭─[ 0:9:111 ]
//    │
//  9 │ function FooToInt(foo: Foo, a: Foo @assert(really_old, {{this.age > 20}})) -> int @check(small_int, {{ this < 10 }}) {
//    │                                                                                                               ─┬  
//    │                                                                                                                ╰── Expected top-level declaration, found IntegerLiteral
//    │ 
//    │ Note: Error code: E0010
// ───╯
// Error: Expected top-level declaration, found RBrace
//    ╭─[ 0:9:114 ]
//    │
//  9 │ function FooToInt(foo: Foo, a: Foo @assert(really_old, {{this.age > 20}})) -> int @check(small_int, {{ this < 10 }}) {
//    │                                                                                                                  ┬  
//    │                                                                                                                  ╰── Expected top-level declaration, found RBrace
//    │ 
//    │ Note: Error code: E0010
// ───╯
// Error: Expected top-level declaration, found RBrace
//    ╭─[ 0:9:115 ]
//    │
//  9 │ function FooToInt(foo: Foo, a: Foo @assert(really_old, {{this.age > 20}})) -> int @check(small_int, {{ this < 10 }}) {
//    │                                                                                                                   ┬  
//    │                                                                                                                   ╰── Expected top-level declaration, found RBrace
//    │ 
//    │ Note: Error code: E0010
// ───╯
// Error: Expected top-level declaration, found RParen
//    ╭─[ 0:9:116 ]
//    │
//  9 │ function FooToInt(foo: Foo, a: Foo @assert(really_old, {{this.age > 20}})) -> int @check(small_int, {{ this < 10 }}) {
//    │                                                                                                                    ┬  
//    │                                                                                                                    ╰── Expected top-level declaration, found RParen
//    │ 
//    │ Note: Error code: E0010
// ───╯
// Error: Expected top-level declaration, found LBrace
//    ╭─[ 0:9:118 ]
//    │
//  9 │ function FooToInt(foo: Foo, a: Foo @assert(really_old, {{this.age > 20}})) -> int @check(small_int, {{ this < 10 }}) {
//    │                                                                                                                      ┬  
//    │                                                                                                                      ╰── Expected top-level declaration, found LBrace
//    │ 
//    │ Note: Error code: E0010
// ───╯
// Error: Expected config block, found Word
//     ╭─[ 0:11:4 ]
//     │
//  11 │    prompt #"fa"#
//     │    ───┬──  
//     │       ╰──── Expected config block, found Word
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found Word
//     ╭─[ 0:11:4 ]
//     │
//  11 │    prompt #"fa"#
//     │    ───┬──  
//     │       ╰──── Expected top-level declaration, found Word
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found Hash
//     ╭─[ 0:11:11 ]
//     │
//  11 │    prompt #"fa"#
//     │           ┬  
//     │           ╰── Expected top-level declaration, found Hash
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found Quote
//     ╭─[ 0:11:12 ]
//     │
//  11 │    prompt #"fa"#
//     │            ┬  
//     │            ╰── Expected top-level declaration, found Quote
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found Word
//     ╭─[ 0:11:13 ]
//     │
//  11 │    prompt #"fa"#
//     │             ─┬  
//     │              ╰── Expected top-level declaration, found Word
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found Quote
//     ╭─[ 0:11:15 ]
//     │
//  11 │    prompt #"fa"#
//     │               ┬  
//     │               ╰── Expected top-level declaration, found Quote
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found Hash
//     ╭─[ 0:11:16 ]
//     │
//  11 │    prompt #"fa"#
//     │                ┬  
//     │                ╰── Expected top-level declaration, found Hash
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
// Error: Duplicate client 'Bar'
//    ╭─[ 0:1:1 ]
//    │
//  1 │ client<llm> Bar {
//    │ │ 
//    │ ╰─ client 'Bar' defined in constraints_constraints_everywhere.baml
//    │ │ 
//    │ ╰─ 'Bar' previously defined in constraints_constraints_everywhere.baml
//    │ 
//    │ Note: Error code: E0011
// ───╯
