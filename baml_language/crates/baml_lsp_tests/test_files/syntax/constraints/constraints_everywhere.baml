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
// Error: Expected ')', found '@'
//    ╭─[ constraints_constraints_everywhere.baml:9:36 ]
//    │
//  9 │ function FooToInt(foo: Foo, a: Foo @assert(really_old, {{this.age > 20}})) -> int @check(small_int, {{ this < 10 }}) {
//    │                                    ┬  
//    │                                    ╰── Expected ')', found '@'
//    │ 
//    │ Note: Error code: E0010
// ───╯
// Error: Expected return type (->), found '@'
//    ╭─[ constraints_constraints_everywhere.baml:9:36 ]
//    │
//  9 │ function FooToInt(foo: Foo, a: Foo @assert(really_old, {{this.age > 20}})) -> int @check(small_int, {{ this < 10 }}) {
//    │                                    ┬  
//    │                                    ╰── Expected return type (->), found '@'
//    │ 
//    │ Note: Error code: E0010
// ───╯
// Error: Expected function body, found '@'
//    ╭─[ constraints_constraints_everywhere.baml:9:36 ]
//    │
//  9 │ function FooToInt(foo: Foo, a: Foo @assert(really_old, {{this.age > 20}})) -> int @check(small_int, {{ this < 10 }}) {
//    │                                    ┬  
//    │                                    ╰── Expected function body, found '@'
//    │ 
//    │ Note: Error code: E0010
// ───╯
// Error: Expected top-level declaration, found '@'
//    ╭─[ constraints_constraints_everywhere.baml:9:36 ]
//    │
//  9 │ function FooToInt(foo: Foo, a: Foo @assert(really_old, {{this.age > 20}})) -> int @check(small_int, {{ this < 10 }}) {
//    │                                    ┬  
//    │                                    ╰── Expected top-level declaration, found '@'
//    │ 
//    │ Note: Error code: E0010
// ───╯
// Error: Expected top-level declaration, found assert
//    ╭─[ constraints_constraints_everywhere.baml:9:37 ]
//    │
//  9 │ function FooToInt(foo: Foo, a: Foo @assert(really_old, {{this.age > 20}})) -> int @check(small_int, {{ this < 10 }}) {
//    │                                     ───┬──  
//    │                                        ╰──── Expected top-level declaration, found assert
//    │ 
//    │ Note: Error code: E0010
// ───╯
// Error: Expected top-level declaration, found '('
//    ╭─[ constraints_constraints_everywhere.baml:9:43 ]
//    │
//  9 │ function FooToInt(foo: Foo, a: Foo @assert(really_old, {{this.age > 20}})) -> int @check(small_int, {{ this < 10 }}) {
//    │                                           ┬  
//    │                                           ╰── Expected top-level declaration, found '('
//    │ 
//    │ Note: Error code: E0010
// ───╯
// Error: Expected top-level declaration, found identifier
//    ╭─[ constraints_constraints_everywhere.baml:9:44 ]
//    │
//  9 │ function FooToInt(foo: Foo, a: Foo @assert(really_old, {{this.age > 20}})) -> int @check(small_int, {{ this < 10 }}) {
//    │                                            ─────┬────  
//    │                                                 ╰────── Expected top-level declaration, found identifier
//    │ 
//    │ Note: Error code: E0010
// ───╯
// Error: Expected top-level declaration, found ','
//    ╭─[ constraints_constraints_everywhere.baml:9:54 ]
//    │
//  9 │ function FooToInt(foo: Foo, a: Foo @assert(really_old, {{this.age > 20}})) -> int @check(small_int, {{ this < 10 }}) {
//    │                                                      ┬  
//    │                                                      ╰── Expected top-level declaration, found ','
//    │ 
//    │ Note: Error code: E0010
// ───╯
// Error: Expected top-level declaration, found '{'
//    ╭─[ constraints_constraints_everywhere.baml:9:56 ]
//    │
//  9 │ function FooToInt(foo: Foo, a: Foo @assert(really_old, {{this.age > 20}})) -> int @check(small_int, {{ this < 10 }}) {
//    │                                                        ┬  
//    │                                                        ╰── Expected top-level declaration, found '{'
//    │ 
//    │ Note: Error code: E0010
// ───╯
// Error: Expected top-level declaration, found '{'
//    ╭─[ constraints_constraints_everywhere.baml:9:57 ]
//    │
//  9 │ function FooToInt(foo: Foo, a: Foo @assert(really_old, {{this.age > 20}})) -> int @check(small_int, {{ this < 10 }}) {
//    │                                                         ┬  
//    │                                                         ╰── Expected top-level declaration, found '{'
//    │ 
//    │ Note: Error code: E0010
// ───╯
// Error: Expected top-level declaration, found identifier
//    ╭─[ constraints_constraints_everywhere.baml:9:58 ]
//    │
//  9 │ function FooToInt(foo: Foo, a: Foo @assert(really_old, {{this.age > 20}})) -> int @check(small_int, {{ this < 10 }}) {
//    │                                                          ──┬─  
//    │                                                            ╰─── Expected top-level declaration, found identifier
//    │ 
//    │ Note: Error code: E0010
// ───╯
// Error: Expected top-level declaration, found '.'
//    ╭─[ constraints_constraints_everywhere.baml:9:62 ]
//    │
//  9 │ function FooToInt(foo: Foo, a: Foo @assert(really_old, {{this.age > 20}})) -> int @check(small_int, {{ this < 10 }}) {
//    │                                                              ┬  
//    │                                                              ╰── Expected top-level declaration, found '.'
//    │ 
//    │ Note: Error code: E0010
// ───╯
// Error: Expected top-level declaration, found identifier
//    ╭─[ constraints_constraints_everywhere.baml:9:63 ]
//    │
//  9 │ function FooToInt(foo: Foo, a: Foo @assert(really_old, {{this.age > 20}})) -> int @check(small_int, {{ this < 10 }}) {
//    │                                                               ─┬─  
//    │                                                                ╰─── Expected top-level declaration, found identifier
//    │ 
//    │ Note: Error code: E0010
// ───╯
// Error: Expected top-level declaration, found '>'
//    ╭─[ constraints_constraints_everywhere.baml:9:67 ]
//    │
//  9 │ function FooToInt(foo: Foo, a: Foo @assert(really_old, {{this.age > 20}})) -> int @check(small_int, {{ this < 10 }}) {
//    │                                                                   ┬  
//    │                                                                   ╰── Expected top-level declaration, found '>'
//    │ 
//    │ Note: Error code: E0010
// ───╯
// Error: Expected top-level declaration, found integer
//    ╭─[ constraints_constraints_everywhere.baml:9:69 ]
//    │
//  9 │ function FooToInt(foo: Foo, a: Foo @assert(really_old, {{this.age > 20}})) -> int @check(small_int, {{ this < 10 }}) {
//    │                                                                     ─┬  
//    │                                                                      ╰── Expected top-level declaration, found integer
//    │ 
//    │ Note: Error code: E0010
// ───╯
// Error: Expected top-level declaration, found '}'
//    ╭─[ constraints_constraints_everywhere.baml:9:71 ]
//    │
//  9 │ function FooToInt(foo: Foo, a: Foo @assert(really_old, {{this.age > 20}})) -> int @check(small_int, {{ this < 10 }}) {
//    │                                                                       ┬  
//    │                                                                       ╰── Expected top-level declaration, found '}'
//    │ 
//    │ Note: Error code: E0010
// ───╯
// Error: Expected top-level declaration, found '}'
//    ╭─[ constraints_constraints_everywhere.baml:9:72 ]
//    │
//  9 │ function FooToInt(foo: Foo, a: Foo @assert(really_old, {{this.age > 20}})) -> int @check(small_int, {{ this < 10 }}) {
//    │                                                                        ┬  
//    │                                                                        ╰── Expected top-level declaration, found '}'
//    │ 
//    │ Note: Error code: E0010
// ───╯
// Error: Expected top-level declaration, found ')'
//    ╭─[ constraints_constraints_everywhere.baml:9:73 ]
//    │
//  9 │ function FooToInt(foo: Foo, a: Foo @assert(really_old, {{this.age > 20}})) -> int @check(small_int, {{ this < 10 }}) {
//    │                                                                         ┬  
//    │                                                                         ╰── Expected top-level declaration, found ')'
//    │ 
//    │ Note: Error code: E0010
// ───╯
// Error: Expected top-level declaration, found ')'
//    ╭─[ constraints_constraints_everywhere.baml:9:74 ]
//    │
//  9 │ function FooToInt(foo: Foo, a: Foo @assert(really_old, {{this.age > 20}})) -> int @check(small_int, {{ this < 10 }}) {
//    │                                                                          ┬  
//    │                                                                          ╰── Expected top-level declaration, found ')'
//    │ 
//    │ Note: Error code: E0010
// ───╯
// Error: Expected top-level declaration, found '->'
//    ╭─[ constraints_constraints_everywhere.baml:9:76 ]
//    │
//  9 │ function FooToInt(foo: Foo, a: Foo @assert(really_old, {{this.age > 20}})) -> int @check(small_int, {{ this < 10 }}) {
//    │                                                                            ─┬  
//    │                                                                             ╰── Expected top-level declaration, found '->'
//    │ 
//    │ Note: Error code: E0010
// ───╯
// Error: Expected top-level declaration, found identifier
//    ╭─[ constraints_constraints_everywhere.baml:9:79 ]
//    │
//  9 │ function FooToInt(foo: Foo, a: Foo @assert(really_old, {{this.age > 20}})) -> int @check(small_int, {{ this < 10 }}) {
//    │                                                                               ─┬─  
//    │                                                                                ╰─── Expected top-level declaration, found identifier
//    │ 
//    │ Note: Error code: E0010
// ───╯
// Error: Expected top-level declaration, found '@'
//    ╭─[ constraints_constraints_everywhere.baml:9:83 ]
//    │
//  9 │ function FooToInt(foo: Foo, a: Foo @assert(really_old, {{this.age > 20}})) -> int @check(small_int, {{ this < 10 }}) {
//    │                                                                                   ┬  
//    │                                                                                   ╰── Expected top-level declaration, found '@'
//    │ 
//    │ Note: Error code: E0010
// ───╯
// Error: Expected top-level declaration, found identifier
//    ╭─[ constraints_constraints_everywhere.baml:9:84 ]
//    │
//  9 │ function FooToInt(foo: Foo, a: Foo @assert(really_old, {{this.age > 20}})) -> int @check(small_int, {{ this < 10 }}) {
//    │                                                                                    ──┬──  
//    │                                                                                      ╰──── Expected top-level declaration, found identifier
//    │ 
//    │ Note: Error code: E0010
// ───╯
// Error: Expected top-level declaration, found '('
//    ╭─[ constraints_constraints_everywhere.baml:9:89 ]
//    │
//  9 │ function FooToInt(foo: Foo, a: Foo @assert(really_old, {{this.age > 20}})) -> int @check(small_int, {{ this < 10 }}) {
//    │                                                                                         ┬  
//    │                                                                                         ╰── Expected top-level declaration, found '('
//    │ 
//    │ Note: Error code: E0010
// ───╯
// Error: Expected top-level declaration, found identifier
//    ╭─[ constraints_constraints_everywhere.baml:9:90 ]
//    │
//  9 │ function FooToInt(foo: Foo, a: Foo @assert(really_old, {{this.age > 20}})) -> int @check(small_int, {{ this < 10 }}) {
//    │                                                                                          ────┬────  
//    │                                                                                              ╰────── Expected top-level declaration, found identifier
//    │ 
//    │ Note: Error code: E0010
// ───╯
// Error: Expected top-level declaration, found ','
//    ╭─[ constraints_constraints_everywhere.baml:9:99 ]
//    │
//  9 │ function FooToInt(foo: Foo, a: Foo @assert(really_old, {{this.age > 20}})) -> int @check(small_int, {{ this < 10 }}) {
//    │                                                                                                   ┬  
//    │                                                                                                   ╰── Expected top-level declaration, found ','
//    │ 
//    │ Note: Error code: E0010
// ───╯
// Error: Expected top-level declaration, found '{'
//    ╭─[ constraints_constraints_everywhere.baml:9:101 ]
//    │
//  9 │ function FooToInt(foo: Foo, a: Foo @assert(really_old, {{this.age > 20}})) -> int @check(small_int, {{ this < 10 }}) {
//    │                                                                                                     ┬  
//    │                                                                                                     ╰── Expected top-level declaration, found '{'
//    │ 
//    │ Note: Error code: E0010
// ───╯
// Error: Expected top-level declaration, found '{'
//    ╭─[ constraints_constraints_everywhere.baml:9:102 ]
//    │
//  9 │ function FooToInt(foo: Foo, a: Foo @assert(really_old, {{this.age > 20}})) -> int @check(small_int, {{ this < 10 }}) {
//    │                                                                                                      ┬  
//    │                                                                                                      ╰── Expected top-level declaration, found '{'
//    │ 
//    │ Note: Error code: E0010
// ───╯
// Error: Expected top-level declaration, found identifier
//    ╭─[ constraints_constraints_everywhere.baml:9:104 ]
//    │
//  9 │ function FooToInt(foo: Foo, a: Foo @assert(really_old, {{this.age > 20}})) -> int @check(small_int, {{ this < 10 }}) {
//    │                                                                                                        ──┬─  
//    │                                                                                                          ╰─── Expected top-level declaration, found identifier
//    │ 
//    │ Note: Error code: E0010
// ───╯
// Error: Expected top-level declaration, found '<'
//    ╭─[ constraints_constraints_everywhere.baml:9:109 ]
//    │
//  9 │ function FooToInt(foo: Foo, a: Foo @assert(really_old, {{this.age > 20}})) -> int @check(small_int, {{ this < 10 }}) {
//    │                                                                                                             ┬  
//    │                                                                                                             ╰── Expected top-level declaration, found '<'
//    │ 
//    │ Note: Error code: E0010
// ───╯
// Error: Expected top-level declaration, found integer
//    ╭─[ constraints_constraints_everywhere.baml:9:111 ]
//    │
//  9 │ function FooToInt(foo: Foo, a: Foo @assert(really_old, {{this.age > 20}})) -> int @check(small_int, {{ this < 10 }}) {
//    │                                                                                                               ─┬  
//    │                                                                                                                ╰── Expected top-level declaration, found integer
//    │ 
//    │ Note: Error code: E0010
// ───╯
// Error: Expected top-level declaration, found '}'
//    ╭─[ constraints_constraints_everywhere.baml:9:114 ]
//    │
//  9 │ function FooToInt(foo: Foo, a: Foo @assert(really_old, {{this.age > 20}})) -> int @check(small_int, {{ this < 10 }}) {
//    │                                                                                                                  ┬  
//    │                                                                                                                  ╰── Expected top-level declaration, found '}'
//    │ 
//    │ Note: Error code: E0010
// ───╯
// Error: Expected top-level declaration, found '}'
//    ╭─[ constraints_constraints_everywhere.baml:9:115 ]
//    │
//  9 │ function FooToInt(foo: Foo, a: Foo @assert(really_old, {{this.age > 20}})) -> int @check(small_int, {{ this < 10 }}) {
//    │                                                                                                                   ┬  
//    │                                                                                                                   ╰── Expected top-level declaration, found '}'
//    │ 
//    │ Note: Error code: E0010
// ───╯
// Error: Expected top-level declaration, found ')'
//    ╭─[ constraints_constraints_everywhere.baml:9:116 ]
//    │
//  9 │ function FooToInt(foo: Foo, a: Foo @assert(really_old, {{this.age > 20}})) -> int @check(small_int, {{ this < 10 }}) {
//    │                                                                                                                    ┬  
//    │                                                                                                                    ╰── Expected top-level declaration, found ')'
//    │ 
//    │ Note: Error code: E0010
// ───╯
// Error: Expected top-level declaration, found '{'
//    ╭─[ constraints_constraints_everywhere.baml:9:118 ]
//    │
//  9 │ function FooToInt(foo: Foo, a: Foo @assert(really_old, {{this.age > 20}})) -> int @check(small_int, {{ this < 10 }}) {
//    │                                                                                                                      ┬  
//    │                                                                                                                      ╰── Expected top-level declaration, found '{'
//    │ 
//    │ Note: Error code: E0010
// ───╯
// Error: Expected config block, found identifier
//     ╭─[ constraints_constraints_everywhere.baml:11:4 ]
//     │
//  11 │    prompt #"fa"#
//     │    ───┬──  
//     │       ╰──── Expected config block, found identifier
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found identifier
//     ╭─[ constraints_constraints_everywhere.baml:11:4 ]
//     │
//  11 │    prompt #"fa"#
//     │    ───┬──  
//     │       ╰──── Expected top-level declaration, found identifier
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found '#'
//     ╭─[ constraints_constraints_everywhere.baml:11:11 ]
//     │
//  11 │    prompt #"fa"#
//     │           ┬  
//     │           ╰── Expected top-level declaration, found '#'
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found '"'
//     ╭─[ constraints_constraints_everywhere.baml:11:12 ]
//     │
//  11 │    prompt #"fa"#
//     │            ┬  
//     │            ╰── Expected top-level declaration, found '"'
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found identifier
//     ╭─[ constraints_constraints_everywhere.baml:11:13 ]
//     │
//  11 │    prompt #"fa"#
//     │             ─┬  
//     │              ╰── Expected top-level declaration, found identifier
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found '"'
//     ╭─[ constraints_constraints_everywhere.baml:11:15 ]
//     │
//  11 │    prompt #"fa"#
//     │               ┬  
//     │               ╰── Expected top-level declaration, found '"'
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found '#'
//     ╭─[ constraints_constraints_everywhere.baml:11:16 ]
//     │
//  11 │    prompt #"fa"#
//     │                ┬  
//     │                ╰── Expected top-level declaration, found '#'
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found '}'
//     ╭─[ constraints_constraints_everywhere.baml:12:1 ]
//     │
//  12 │ }
//     │ ┬  
//     │ ╰── Expected top-level declaration, found '}'
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Missing `provider` field in client. e.g. `provider openai`
//     ╭─[ constraints_constraints_everywhere.baml:9:119 ]
//     │
//   9 │ ╭─▶ function FooToInt(foo: Foo, a: Foo @assert(really_old, {{this.age > 20}})) -> int @check(small_int, {{ this < 10 }}) {
//  10 │ ├─▶    client Bar
//     │ │                   
//     │ ╰─────────────────── Missing `provider` field in client. e.g. `provider openai`
//     │     
//     │     Note: Error code: E0026
// ────╯
// Error: Duplicate client 'Bar'
//    ╭─[ constraints_constraints_everywhere.baml:1:1 ]
//    │
//  1 │ client<llm> Bar {
//    │ │ 
//    │ ╰─ client 'Bar' defined in constraints_constraints_everywhere.baml
//    │ │ 
//    │ ╰─ 'Bar' previously defined in constraints_constraints_everywhere.baml
//    │ 
//    │ Note: Error code: E0011
// ───╯
