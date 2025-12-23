class Foo {
  foo int
  bar string
  @@check(foo_and_bar, {{ this.string|length < this.foo }})
}

class Foo2 {
  foo int
  bar string
  @@assert({{ this.string|length < this.foo }})
}

class Foo3 {
  foo int @check(foo_check, {{ this }})
  bar string @assert(hi, {{ this }}) @check(hi, {{ this }})
  @@assert({{ this.string|length < this.foo }})
  @@check(foo_check_2, {{ this }})
}

client<llm> GPT35 {
  provider baml-openai-chat
  options {
    model gpt-3.5-turbo
    max_tokens 100
  }
}

function FooFn(input: Foo2) -> Foo {
  client GPT35
  prompt #"
  "#
} 

test TestFoo {
  functions [FooFn]
  args {}
  @@check(foo_check_1, {{ this }})
  @@check(foo_check_2, {{ this }})
}

//----
//- diagnostics
// Error: Expected config key, found AtAt
//     ╭─[ 0:37:3 ]
//     │
//  37 │   @@check(foo_check_1, {{ this }})
//     │   ─┬  
//     │    ╰── Expected config key, found AtAt
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected config key, found Comma
//     ╭─[ 0:37:22 ]
//     │
//  37 │   @@check(foo_check_1, {{ this }})
//     │                      ┬  
//     │                      ╰── Expected config key, found Comma
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected config key, found LBrace
//     ╭─[ 0:37:24 ]
//     │
//  37 │   @@check(foo_check_1, {{ this }})
//     │                        ┬  
//     │                        ╰── Expected config key, found LBrace
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected config key, found LBrace
//     ╭─[ 0:37:25 ]
//     │
//  37 │   @@check(foo_check_1, {{ this }})
//     │                         ┬  
//     │                         ╰── Expected config key, found LBrace
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found RBrace
//     ╭─[ 0:37:33 ]
//     │
//  37 │   @@check(foo_check_1, {{ this }})
//     │                                 ┬  
//     │                                 ╰── Expected top-level declaration, found RBrace
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found RParen
//     ╭─[ 0:37:34 ]
//     │
//  37 │   @@check(foo_check_1, {{ this }})
//     │                                  ┬  
//     │                                  ╰── Expected top-level declaration, found RParen
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found AtAt
//     ╭─[ 0:38:3 ]
//     │
//  38 │   @@check(foo_check_2, {{ this }})
//     │   ─┬  
//     │    ╰── Expected top-level declaration, found AtAt
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found Word
//     ╭─[ 0:38:5 ]
//     │
//  38 │   @@check(foo_check_2, {{ this }})
//     │     ──┬──  
//     │       ╰──── Expected top-level declaration, found Word
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found LParen
//     ╭─[ 0:38:10 ]
//     │
//  38 │   @@check(foo_check_2, {{ this }})
//     │          ┬  
//     │          ╰── Expected top-level declaration, found LParen
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found Word
//     ╭─[ 0:38:11 ]
//     │
//  38 │   @@check(foo_check_2, {{ this }})
//     │           ─────┬─────  
//     │                ╰─────── Expected top-level declaration, found Word
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found Comma
//     ╭─[ 0:38:22 ]
//     │
//  38 │   @@check(foo_check_2, {{ this }})
//     │                      ┬  
//     │                      ╰── Expected top-level declaration, found Comma
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found LBrace
//     ╭─[ 0:38:24 ]
//     │
//  38 │   @@check(foo_check_2, {{ this }})
//     │                        ┬  
//     │                        ╰── Expected top-level declaration, found LBrace
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found LBrace
//     ╭─[ 0:38:25 ]
//     │
//  38 │   @@check(foo_check_2, {{ this }})
//     │                         ┬  
//     │                         ╰── Expected top-level declaration, found LBrace
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found Word
//     ╭─[ 0:38:27 ]
//     │
//  38 │   @@check(foo_check_2, {{ this }})
//     │                           ──┬─  
//     │                             ╰─── Expected top-level declaration, found Word
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found RBrace
//     ╭─[ 0:38:32 ]
//     │
//  38 │   @@check(foo_check_2, {{ this }})
//     │                                ┬  
//     │                                ╰── Expected top-level declaration, found RBrace
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found RBrace
//     ╭─[ 0:38:33 ]
//     │
//  38 │   @@check(foo_check_2, {{ this }})
//     │                                 ┬  
//     │                                 ╰── Expected top-level declaration, found RBrace
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found RParen
//     ╭─[ 0:38:34 ]
//     │
//  38 │   @@check(foo_check_2, {{ this }})
//     │                                  ┬  
//     │                                  ╰── Expected top-level declaration, found RParen
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found RBrace
//     ╭─[ 0:39:1 ]
//     │
//  39 │ }
//     │ ┬  
//     │ ╰── Expected top-level declaration, found RBrace
//     │ 
//     │ Note: Error code: E0010
// ────╯
