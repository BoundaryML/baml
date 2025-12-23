function InputNone() -> int {
  client "openai/gpt-4o"
  prompt #"fa"#
}

function InputSimple(a: string, b: int | bool) -> int {
  client "openai/gpt-4o"
  prompt #"fa"#
}

class Email {
    subject string
    body string
}

function InputObject(email: Email) -> string {
  client "openai/gpt-4o"
  prompt #" subject line {{ email.subject }} "#
}

enum Color {
  RED
  BLUE
  GREEN
}

function InputEnum(color: Color) -> string {
  client "openai/gpt-4o"
  prompt #" color {{ color }} "#
}

// Try it with wierd spacing and comments

function InputEnum2(
  color: Color,
  bar: string, // ok

  // This param is great!
  foo: string[],

  // But we should do something...
  baz: (string | int?)[]
)
// Err
->
// Do something
string

// Or else
{
  client "openai/gpt-4o"
  prompt #" color {{ color }} "#
}


function InputImage(img: image) -> string {
  client "openai/gpt-4o"
  prompt #" image {{ img }} "#
}

//----
//- diagnostics
// Error: Expected client name, found Quote
//    ╭─[ 0:2:10 ]
//    │
//  2 │   client "openai/gpt-4o"
//    │          ┬  
//    │          ╰── Expected client name, found Quote
//    │ 
//    │ Note: Error code: E0010
// ───╯
// Error: Expected Only 'client' and 'prompt' allowed in LLM function, found '"', found Quote
//    ╭─[ 0:2:10 ]
//    │
//  2 │   client "openai/gpt-4o"
//    │          ┬  
//    │          ╰── Expected Only 'client' and 'prompt' allowed in LLM function, found '"', found Quote
//    │ 
//    │ Note: Error code: E0010
// ───╯
// Error: Expected Only 'client' and 'prompt' allowed in LLM function, found 'openai', found Word
//    ╭─[ 0:2:11 ]
//    │
//  2 │   client "openai/gpt-4o"
//    │           ───┬──  
//    │              ╰──── Expected Only 'client' and 'prompt' allowed in LLM function, found 'openai', found Word
//    │ 
//    │ Note: Error code: E0010
// ───╯
// Error: Expected Only 'client' and 'prompt' allowed in LLM function, found '/', found Slash
//    ╭─[ 0:2:17 ]
//    │
//  2 │   client "openai/gpt-4o"
//    │                 ┬  
//    │                 ╰── Expected Only 'client' and 'prompt' allowed in LLM function, found '/', found Slash
//    │ 
//    │ Note: Error code: E0010
// ───╯
// Error: Expected Only 'client' and 'prompt' allowed in LLM function, found 'gpt-4o', found Word
//    ╭─[ 0:2:18 ]
//    │
//  2 │   client "openai/gpt-4o"
//    │                  ───┬──  
//    │                     ╰──── Expected Only 'client' and 'prompt' allowed in LLM function, found 'gpt-4o', found Word
//    │ 
//    │ Note: Error code: E0010
// ───╯
// Error: Expected Only 'client' and 'prompt' allowed in LLM function, found '"', found Quote
//    ╭─[ 0:2:24 ]
//    │
//  2 │   client "openai/gpt-4o"
//    │                        ┬  
//    │                        ╰── Expected Only 'client' and 'prompt' allowed in LLM function, found '"', found Quote
//    │ 
//    │ Note: Error code: E0010
// ───╯
// Error: Expected client name, found Quote
//    ╭─[ 0:7:10 ]
//    │
//  7 │   client "openai/gpt-4o"
//    │          ┬  
//    │          ╰── Expected client name, found Quote
//    │ 
//    │ Note: Error code: E0010
// ───╯
// Error: Expected Only 'client' and 'prompt' allowed in LLM function, found '"', found Quote
//    ╭─[ 0:7:10 ]
//    │
//  7 │   client "openai/gpt-4o"
//    │          ┬  
//    │          ╰── Expected Only 'client' and 'prompt' allowed in LLM function, found '"', found Quote
//    │ 
//    │ Note: Error code: E0010
// ───╯
// Error: Expected Only 'client' and 'prompt' allowed in LLM function, found 'openai', found Word
//    ╭─[ 0:7:11 ]
//    │
//  7 │   client "openai/gpt-4o"
//    │           ───┬──  
//    │              ╰──── Expected Only 'client' and 'prompt' allowed in LLM function, found 'openai', found Word
//    │ 
//    │ Note: Error code: E0010
// ───╯
// Error: Expected Only 'client' and 'prompt' allowed in LLM function, found '/', found Slash
//    ╭─[ 0:7:17 ]
//    │
//  7 │   client "openai/gpt-4o"
//    │                 ┬  
//    │                 ╰── Expected Only 'client' and 'prompt' allowed in LLM function, found '/', found Slash
//    │ 
//    │ Note: Error code: E0010
// ───╯
// Error: Expected Only 'client' and 'prompt' allowed in LLM function, found 'gpt-4o', found Word
//    ╭─[ 0:7:18 ]
//    │
//  7 │   client "openai/gpt-4o"
//    │                  ───┬──  
//    │                     ╰──── Expected Only 'client' and 'prompt' allowed in LLM function, found 'gpt-4o', found Word
//    │ 
//    │ Note: Error code: E0010
// ───╯
// Error: Expected Only 'client' and 'prompt' allowed in LLM function, found '"', found Quote
//    ╭─[ 0:7:24 ]
//    │
//  7 │   client "openai/gpt-4o"
//    │                        ┬  
//    │                        ╰── Expected Only 'client' and 'prompt' allowed in LLM function, found '"', found Quote
//    │ 
//    │ Note: Error code: E0010
// ───╯
// Error: Expected client name, found Quote
//     ╭─[ 0:17:10 ]
//     │
//  17 │   client "openai/gpt-4o"
//     │          ┬  
//     │          ╰── Expected client name, found Quote
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected Only 'client' and 'prompt' allowed in LLM function, found '"', found Quote
//     ╭─[ 0:17:10 ]
//     │
//  17 │   client "openai/gpt-4o"
//     │          ┬  
//     │          ╰── Expected Only 'client' and 'prompt' allowed in LLM function, found '"', found Quote
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected Only 'client' and 'prompt' allowed in LLM function, found 'openai', found Word
//     ╭─[ 0:17:11 ]
//     │
//  17 │   client "openai/gpt-4o"
//     │           ───┬──  
//     │              ╰──── Expected Only 'client' and 'prompt' allowed in LLM function, found 'openai', found Word
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected Only 'client' and 'prompt' allowed in LLM function, found '/', found Slash
//     ╭─[ 0:17:17 ]
//     │
//  17 │   client "openai/gpt-4o"
//     │                 ┬  
//     │                 ╰── Expected Only 'client' and 'prompt' allowed in LLM function, found '/', found Slash
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected Only 'client' and 'prompt' allowed in LLM function, found 'gpt-4o', found Word
//     ╭─[ 0:17:18 ]
//     │
//  17 │   client "openai/gpt-4o"
//     │                  ───┬──  
//     │                     ╰──── Expected Only 'client' and 'prompt' allowed in LLM function, found 'gpt-4o', found Word
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected Only 'client' and 'prompt' allowed in LLM function, found '"', found Quote
//     ╭─[ 0:17:24 ]
//     │
//  17 │   client "openai/gpt-4o"
//     │                        ┬  
//     │                        ╰── Expected Only 'client' and 'prompt' allowed in LLM function, found '"', found Quote
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected client name, found Quote
//     ╭─[ 0:28:10 ]
//     │
//  28 │   client "openai/gpt-4o"
//     │          ┬  
//     │          ╰── Expected client name, found Quote
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected Only 'client' and 'prompt' allowed in LLM function, found '"', found Quote
//     ╭─[ 0:28:10 ]
//     │
//  28 │   client "openai/gpt-4o"
//     │          ┬  
//     │          ╰── Expected Only 'client' and 'prompt' allowed in LLM function, found '"', found Quote
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected Only 'client' and 'prompt' allowed in LLM function, found 'openai', found Word
//     ╭─[ 0:28:11 ]
//     │
//  28 │   client "openai/gpt-4o"
//     │           ───┬──  
//     │              ╰──── Expected Only 'client' and 'prompt' allowed in LLM function, found 'openai', found Word
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected Only 'client' and 'prompt' allowed in LLM function, found '/', found Slash
//     ╭─[ 0:28:17 ]
//     │
//  28 │   client "openai/gpt-4o"
//     │                 ┬  
//     │                 ╰── Expected Only 'client' and 'prompt' allowed in LLM function, found '/', found Slash
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected Only 'client' and 'prompt' allowed in LLM function, found 'gpt-4o', found Word
//     ╭─[ 0:28:18 ]
//     │
//  28 │   client "openai/gpt-4o"
//     │                  ───┬──  
//     │                     ╰──── Expected Only 'client' and 'prompt' allowed in LLM function, found 'gpt-4o', found Word
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected Only 'client' and 'prompt' allowed in LLM function, found '"', found Quote
//     ╭─[ 0:28:24 ]
//     │
//  28 │   client "openai/gpt-4o"
//     │                        ┬  
//     │                        ╰── Expected Only 'client' and 'prompt' allowed in LLM function, found '"', found Quote
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected client name, found Quote
//     ╭─[ 0:51:10 ]
//     │
//  51 │   client "openai/gpt-4o"
//     │          ┬  
//     │          ╰── Expected client name, found Quote
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected Only 'client' and 'prompt' allowed in LLM function, found '"', found Quote
//     ╭─[ 0:51:10 ]
//     │
//  51 │   client "openai/gpt-4o"
//     │          ┬  
//     │          ╰── Expected Only 'client' and 'prompt' allowed in LLM function, found '"', found Quote
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected Only 'client' and 'prompt' allowed in LLM function, found 'openai', found Word
//     ╭─[ 0:51:11 ]
//     │
//  51 │   client "openai/gpt-4o"
//     │           ───┬──  
//     │              ╰──── Expected Only 'client' and 'prompt' allowed in LLM function, found 'openai', found Word
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected Only 'client' and 'prompt' allowed in LLM function, found '/', found Slash
//     ╭─[ 0:51:17 ]
//     │
//  51 │   client "openai/gpt-4o"
//     │                 ┬  
//     │                 ╰── Expected Only 'client' and 'prompt' allowed in LLM function, found '/', found Slash
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected Only 'client' and 'prompt' allowed in LLM function, found 'gpt-4o', found Word
//     ╭─[ 0:51:18 ]
//     │
//  51 │   client "openai/gpt-4o"
//     │                  ───┬──  
//     │                     ╰──── Expected Only 'client' and 'prompt' allowed in LLM function, found 'gpt-4o', found Word
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected Only 'client' and 'prompt' allowed in LLM function, found '"', found Quote
//     ╭─[ 0:51:24 ]
//     │
//  51 │   client "openai/gpt-4o"
//     │                        ┬  
//     │                        ╰── Expected Only 'client' and 'prompt' allowed in LLM function, found '"', found Quote
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected client name, found Quote
//     ╭─[ 0:57:10 ]
//     │
//  57 │   client "openai/gpt-4o"
//     │          ┬  
//     │          ╰── Expected client name, found Quote
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected Only 'client' and 'prompt' allowed in LLM function, found '"', found Quote
//     ╭─[ 0:57:10 ]
//     │
//  57 │   client "openai/gpt-4o"
//     │          ┬  
//     │          ╰── Expected Only 'client' and 'prompt' allowed in LLM function, found '"', found Quote
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected Only 'client' and 'prompt' allowed in LLM function, found 'openai', found Word
//     ╭─[ 0:57:11 ]
//     │
//  57 │   client "openai/gpt-4o"
//     │           ───┬──  
//     │              ╰──── Expected Only 'client' and 'prompt' allowed in LLM function, found 'openai', found Word
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected Only 'client' and 'prompt' allowed in LLM function, found '/', found Slash
//     ╭─[ 0:57:17 ]
//     │
//  57 │   client "openai/gpt-4o"
//     │                 ┬  
//     │                 ╰── Expected Only 'client' and 'prompt' allowed in LLM function, found '/', found Slash
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected Only 'client' and 'prompt' allowed in LLM function, found 'gpt-4o', found Word
//     ╭─[ 0:57:18 ]
//     │
//  57 │   client "openai/gpt-4o"
//     │                  ───┬──  
//     │                     ╰──── Expected Only 'client' and 'prompt' allowed in LLM function, found 'gpt-4o', found Word
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected Only 'client' and 'prompt' allowed in LLM function, found '"', found Quote
//     ╭─[ 0:57:24 ]
//     │
//  57 │   client "openai/gpt-4o"
//     │                        ┬  
//     │                        ╰── Expected Only 'client' and 'prompt' allowed in LLM function, found '"', found Quote
//     │ 
//     │ Note: Error code: E0010
// ────╯
