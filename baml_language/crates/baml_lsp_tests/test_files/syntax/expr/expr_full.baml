function MakePoem(length: int) -> string {
    client GPT4o
    prompt #"Write a poem {{ length }} lines long."#
}

function CombinePoems(poem1: string, poem2: string) -> string {
    client GPT4o
    prompt #"Combine the following two poems into one poem.

    Poem 1:
    {{ poem1 }}

    Poem 2:
    {{ poem2 }}
    "#
}

let poem = MakePoem(10);

let another = {
  let x = MakePoem(10);
  let y = MakePoem(5);
  CombinePoems(x,y)
};

function Pipeline() -> string {
    let x = MakePoem(6);
    let y = MakePoem(6);
    let a = MakePoem(6);
    let b = MakePoem(6);
    let xy = CombinePoems(x,y);
    let ab = CombinePoems(a,b);
    CombinePoems(xy, ab)
}

function Pyramid() -> string {
  CombinePoems( CombinePoems( MakePoem(10), MakePoem(10)), MakePoem(10))
}

function OuterPyramid() -> string {
  CombinePoems(poem, another)
}

test TestPipeline() {
  functions [Pipeline]
  args { }
}

test TestPyramid() {
  functions [Pyramid]
  args { }
}

test OuterPyramid() {
  functions [OuterPyramid]
  args { }
}

client<llm> GPT4o {
  provider openai
  options {
    model gpt-4o
    api_key env.OPENAI_API_KEY
  }
}

test TestMakePoem() {
    functions [MakePoem]
    args { length 4 }
}

//----
//- diagnostics
// Error: Expected top-level declaration, found Let
//     ╭─[ 0:18:1 ]
//     │
//  18 │ let poem = MakePoem(10);
//     │ ─┬─  
//     │  ╰─── Expected top-level declaration, found Let
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found Word
//     ╭─[ 0:18:5 ]
//     │
//  18 │ let poem = MakePoem(10);
//     │     ──┬─  
//     │       ╰─── Expected top-level declaration, found Word
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found Equals
//     ╭─[ 0:18:10 ]
//     │
//  18 │ let poem = MakePoem(10);
//     │          ┬  
//     │          ╰── Expected top-level declaration, found Equals
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found Word
//     ╭─[ 0:18:12 ]
//     │
//  18 │ let poem = MakePoem(10);
//     │            ────┬───  
//     │                ╰───── Expected top-level declaration, found Word
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found LParen
//     ╭─[ 0:18:20 ]
//     │
//  18 │ let poem = MakePoem(10);
//     │                    ┬  
//     │                    ╰── Expected top-level declaration, found LParen
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found IntegerLiteral
//     ╭─[ 0:18:21 ]
//     │
//  18 │ let poem = MakePoem(10);
//     │                     ─┬  
//     │                      ╰── Expected top-level declaration, found IntegerLiteral
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found RParen
//     ╭─[ 0:18:23 ]
//     │
//  18 │ let poem = MakePoem(10);
//     │                       ┬  
//     │                       ╰── Expected top-level declaration, found RParen
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found Semicolon
//     ╭─[ 0:18:24 ]
//     │
//  18 │ let poem = MakePoem(10);
//     │                        ┬  
//     │                        ╰── Expected top-level declaration, found Semicolon
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found Let
//     ╭─[ 0:20:1 ]
//     │
//  20 │ let another = {
//     │ ─┬─  
//     │  ╰─── Expected top-level declaration, found Let
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found Word
//     ╭─[ 0:20:5 ]
//     │
//  20 │ let another = {
//     │     ───┬───  
//     │        ╰───── Expected top-level declaration, found Word
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found Equals
//     ╭─[ 0:20:13 ]
//     │
//  20 │ let another = {
//     │             ┬  
//     │             ╰── Expected top-level declaration, found Equals
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found LBrace
//     ╭─[ 0:20:15 ]
//     │
//  20 │ let another = {
//     │               ┬  
//     │               ╰── Expected top-level declaration, found LBrace
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found Let
//     ╭─[ 0:21:3 ]
//     │
//  21 │   let x = MakePoem(10);
//     │   ─┬─  
//     │    ╰─── Expected top-level declaration, found Let
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found Word
//     ╭─[ 0:21:7 ]
//     │
//  21 │   let x = MakePoem(10);
//     │       ┬  
//     │       ╰── Expected top-level declaration, found Word
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found Equals
//     ╭─[ 0:21:9 ]
//     │
//  21 │   let x = MakePoem(10);
//     │         ┬  
//     │         ╰── Expected top-level declaration, found Equals
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found Word
//     ╭─[ 0:21:11 ]
//     │
//  21 │   let x = MakePoem(10);
//     │           ────┬───  
//     │               ╰───── Expected top-level declaration, found Word
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found LParen
//     ╭─[ 0:21:19 ]
//     │
//  21 │   let x = MakePoem(10);
//     │                   ┬  
//     │                   ╰── Expected top-level declaration, found LParen
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found IntegerLiteral
//     ╭─[ 0:21:20 ]
//     │
//  21 │   let x = MakePoem(10);
//     │                    ─┬  
//     │                     ╰── Expected top-level declaration, found IntegerLiteral
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found RParen
//     ╭─[ 0:21:22 ]
//     │
//  21 │   let x = MakePoem(10);
//     │                      ┬  
//     │                      ╰── Expected top-level declaration, found RParen
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found Semicolon
//     ╭─[ 0:21:23 ]
//     │
//  21 │   let x = MakePoem(10);
//     │                       ┬  
//     │                       ╰── Expected top-level declaration, found Semicolon
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found Let
//     ╭─[ 0:22:3 ]
//     │
//  22 │   let y = MakePoem(5);
//     │   ─┬─  
//     │    ╰─── Expected top-level declaration, found Let
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found Word
//     ╭─[ 0:22:7 ]
//     │
//  22 │   let y = MakePoem(5);
//     │       ┬  
//     │       ╰── Expected top-level declaration, found Word
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found Equals
//     ╭─[ 0:22:9 ]
//     │
//  22 │   let y = MakePoem(5);
//     │         ┬  
//     │         ╰── Expected top-level declaration, found Equals
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found Word
//     ╭─[ 0:22:11 ]
//     │
//  22 │   let y = MakePoem(5);
//     │           ────┬───  
//     │               ╰───── Expected top-level declaration, found Word
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found LParen
//     ╭─[ 0:22:19 ]
//     │
//  22 │   let y = MakePoem(5);
//     │                   ┬  
//     │                   ╰── Expected top-level declaration, found LParen
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found IntegerLiteral
//     ╭─[ 0:22:20 ]
//     │
//  22 │   let y = MakePoem(5);
//     │                    ┬  
//     │                    ╰── Expected top-level declaration, found IntegerLiteral
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found RParen
//     ╭─[ 0:22:21 ]
//     │
//  22 │   let y = MakePoem(5);
//     │                     ┬  
//     │                     ╰── Expected top-level declaration, found RParen
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found Semicolon
//     ╭─[ 0:22:22 ]
//     │
//  22 │   let y = MakePoem(5);
//     │                      ┬  
//     │                      ╰── Expected top-level declaration, found Semicolon
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found Word
//     ╭─[ 0:23:3 ]
//     │
//  23 │   CombinePoems(x,y)
//     │   ──────┬─────  
//     │         ╰─────── Expected top-level declaration, found Word
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found LParen
//     ╭─[ 0:23:15 ]
//     │
//  23 │   CombinePoems(x,y)
//     │               ┬  
//     │               ╰── Expected top-level declaration, found LParen
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found Word
//     ╭─[ 0:23:16 ]
//     │
//  23 │   CombinePoems(x,y)
//     │                ┬  
//     │                ╰── Expected top-level declaration, found Word
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found Comma
//     ╭─[ 0:23:17 ]
//     │
//  23 │   CombinePoems(x,y)
//     │                 ┬  
//     │                 ╰── Expected top-level declaration, found Comma
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found Word
//     ╭─[ 0:23:18 ]
//     │
//  23 │   CombinePoems(x,y)
//     │                  ┬  
//     │                  ╰── Expected top-level declaration, found Word
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found RParen
//     ╭─[ 0:23:19 ]
//     │
//  23 │   CombinePoems(x,y)
//     │                   ┬  
//     │                   ╰── Expected top-level declaration, found RParen
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found RBrace
//     ╭─[ 0:24:1 ]
//     │
//  24 │ };
//     │ ┬  
//     │ ╰── Expected top-level declaration, found RBrace
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found Semicolon
//     ╭─[ 0:24:2 ]
//     │
//  24 │ };
//     │  ┬  
//     │  ╰── Expected top-level declaration, found Semicolon
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected test body, found LParen
//     ╭─[ 0:44:18 ]
//     │
//  44 │ test TestPipeline() {
//     │                  ┬  
//     │                  ╰── Expected test body, found LParen
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found LParen
//     ╭─[ 0:44:18 ]
//     │
//  44 │ test TestPipeline() {
//     │                  ┬  
//     │                  ╰── Expected top-level declaration, found LParen
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found RParen
//     ╭─[ 0:44:19 ]
//     │
//  44 │ test TestPipeline() {
//     │                   ┬  
//     │                   ╰── Expected top-level declaration, found RParen
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found LBrace
//     ╭─[ 0:44:21 ]
//     │
//  44 │ test TestPipeline() {
//     │                     ┬  
//     │                     ╰── Expected top-level declaration, found LBrace
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found Word
//     ╭─[ 0:45:3 ]
//     │
//  45 │   functions [Pipeline]
//     │   ────┬────  
//     │       ╰────── Expected top-level declaration, found Word
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found LBracket
//     ╭─[ 0:45:13 ]
//     │
//  45 │   functions [Pipeline]
//     │             ┬  
//     │             ╰── Expected top-level declaration, found LBracket
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found Word
//     ╭─[ 0:45:14 ]
//     │
//  45 │   functions [Pipeline]
//     │              ────┬───  
//     │                  ╰───── Expected top-level declaration, found Word
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found RBracket
//     ╭─[ 0:45:22 ]
//     │
//  45 │   functions [Pipeline]
//     │                      ┬  
//     │                      ╰── Expected top-level declaration, found RBracket
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found Word
//     ╭─[ 0:46:3 ]
//     │
//  46 │   args { }
//     │   ──┬─  
//     │     ╰─── Expected top-level declaration, found Word
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found LBrace
//     ╭─[ 0:46:8 ]
//     │
//  46 │   args { }
//     │        ┬  
//     │        ╰── Expected top-level declaration, found LBrace
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found RBrace
//     ╭─[ 0:46:10 ]
//     │
//  46 │   args { }
//     │          ┬  
//     │          ╰── Expected top-level declaration, found RBrace
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found RBrace
//     ╭─[ 0:47:1 ]
//     │
//  47 │ }
//     │ ┬  
//     │ ╰── Expected top-level declaration, found RBrace
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected test body, found LParen
//     ╭─[ 0:49:17 ]
//     │
//  49 │ test TestPyramid() {
//     │                 ┬  
//     │                 ╰── Expected test body, found LParen
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found LParen
//     ╭─[ 0:49:17 ]
//     │
//  49 │ test TestPyramid() {
//     │                 ┬  
//     │                 ╰── Expected top-level declaration, found LParen
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found RParen
//     ╭─[ 0:49:18 ]
//     │
//  49 │ test TestPyramid() {
//     │                  ┬  
//     │                  ╰── Expected top-level declaration, found RParen
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found LBrace
//     ╭─[ 0:49:20 ]
//     │
//  49 │ test TestPyramid() {
//     │                    ┬  
//     │                    ╰── Expected top-level declaration, found LBrace
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found Word
//     ╭─[ 0:50:3 ]
//     │
//  50 │   functions [Pyramid]
//     │   ────┬────  
//     │       ╰────── Expected top-level declaration, found Word
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found LBracket
//     ╭─[ 0:50:13 ]
//     │
//  50 │   functions [Pyramid]
//     │             ┬  
//     │             ╰── Expected top-level declaration, found LBracket
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found Word
//     ╭─[ 0:50:14 ]
//     │
//  50 │   functions [Pyramid]
//     │              ───┬───  
//     │                 ╰───── Expected top-level declaration, found Word
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found RBracket
//     ╭─[ 0:50:21 ]
//     │
//  50 │   functions [Pyramid]
//     │                     ┬  
//     │                     ╰── Expected top-level declaration, found RBracket
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found Word
//     ╭─[ 0:51:3 ]
//     │
//  51 │   args { }
//     │   ──┬─  
//     │     ╰─── Expected top-level declaration, found Word
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found LBrace
//     ╭─[ 0:51:8 ]
//     │
//  51 │   args { }
//     │        ┬  
//     │        ╰── Expected top-level declaration, found LBrace
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found RBrace
//     ╭─[ 0:51:10 ]
//     │
//  51 │   args { }
//     │          ┬  
//     │          ╰── Expected top-level declaration, found RBrace
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found RBrace
//     ╭─[ 0:52:1 ]
//     │
//  52 │ }
//     │ ┬  
//     │ ╰── Expected top-level declaration, found RBrace
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected test body, found LParen
//     ╭─[ 0:54:18 ]
//     │
//  54 │ test OuterPyramid() {
//     │                  ┬  
//     │                  ╰── Expected test body, found LParen
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found LParen
//     ╭─[ 0:54:18 ]
//     │
//  54 │ test OuterPyramid() {
//     │                  ┬  
//     │                  ╰── Expected top-level declaration, found LParen
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found RParen
//     ╭─[ 0:54:19 ]
//     │
//  54 │ test OuterPyramid() {
//     │                   ┬  
//     │                   ╰── Expected top-level declaration, found RParen
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found LBrace
//     ╭─[ 0:54:21 ]
//     │
//  54 │ test OuterPyramid() {
//     │                     ┬  
//     │                     ╰── Expected top-level declaration, found LBrace
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found Word
//     ╭─[ 0:55:3 ]
//     │
//  55 │   functions [OuterPyramid]
//     │   ────┬────  
//     │       ╰────── Expected top-level declaration, found Word
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found LBracket
//     ╭─[ 0:55:13 ]
//     │
//  55 │   functions [OuterPyramid]
//     │             ┬  
//     │             ╰── Expected top-level declaration, found LBracket
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found Word
//     ╭─[ 0:55:14 ]
//     │
//  55 │   functions [OuterPyramid]
//     │              ──────┬─────  
//     │                    ╰─────── Expected top-level declaration, found Word
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found RBracket
//     ╭─[ 0:55:26 ]
//     │
//  55 │   functions [OuterPyramid]
//     │                          ┬  
//     │                          ╰── Expected top-level declaration, found RBracket
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found Word
//     ╭─[ 0:56:3 ]
//     │
//  56 │   args { }
//     │   ──┬─  
//     │     ╰─── Expected top-level declaration, found Word
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found LBrace
//     ╭─[ 0:56:8 ]
//     │
//  56 │   args { }
//     │        ┬  
//     │        ╰── Expected top-level declaration, found LBrace
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found RBrace
//     ╭─[ 0:56:10 ]
//     │
//  56 │   args { }
//     │          ┬  
//     │          ╰── Expected top-level declaration, found RBrace
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found RBrace
//     ╭─[ 0:57:1 ]
//     │
//  57 │ }
//     │ ┬  
//     │ ╰── Expected top-level declaration, found RBrace
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected test body, found LParen
//     ╭─[ 0:67:18 ]
//     │
//  67 │ test TestMakePoem() {
//     │                  ┬  
//     │                  ╰── Expected test body, found LParen
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found LParen
//     ╭─[ 0:67:18 ]
//     │
//  67 │ test TestMakePoem() {
//     │                  ┬  
//     │                  ╰── Expected top-level declaration, found LParen
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found RParen
//     ╭─[ 0:67:19 ]
//     │
//  67 │ test TestMakePoem() {
//     │                   ┬  
//     │                   ╰── Expected top-level declaration, found RParen
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found LBrace
//     ╭─[ 0:67:21 ]
//     │
//  67 │ test TestMakePoem() {
//     │                     ┬  
//     │                     ╰── Expected top-level declaration, found LBrace
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found Word
//     ╭─[ 0:68:5 ]
//     │
//  68 │     functions [MakePoem]
//     │     ────┬────  
//     │         ╰────── Expected top-level declaration, found Word
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found LBracket
//     ╭─[ 0:68:15 ]
//     │
//  68 │     functions [MakePoem]
//     │               ┬  
//     │               ╰── Expected top-level declaration, found LBracket
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found Word
//     ╭─[ 0:68:16 ]
//     │
//  68 │     functions [MakePoem]
//     │                ────┬───  
//     │                    ╰───── Expected top-level declaration, found Word
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found RBracket
//     ╭─[ 0:68:24 ]
//     │
//  68 │     functions [MakePoem]
//     │                        ┬  
//     │                        ╰── Expected top-level declaration, found RBracket
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found Word
//     ╭─[ 0:69:5 ]
//     │
//  69 │     args { length 4 }
//     │     ──┬─  
//     │       ╰─── Expected top-level declaration, found Word
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found LBrace
//     ╭─[ 0:69:10 ]
//     │
//  69 │     args { length 4 }
//     │          ┬  
//     │          ╰── Expected top-level declaration, found LBrace
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found Word
//     ╭─[ 0:69:12 ]
//     │
//  69 │     args { length 4 }
//     │            ───┬──  
//     │               ╰──── Expected top-level declaration, found Word
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found IntegerLiteral
//     ╭─[ 0:69:19 ]
//     │
//  69 │     args { length 4 }
//     │                   ┬  
//     │                   ╰── Expected top-level declaration, found IntegerLiteral
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found RBrace
//     ╭─[ 0:69:21 ]
//     │
//  69 │     args { length 4 }
//     │                     ┬  
//     │                     ╰── Expected top-level declaration, found RBrace
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found RBrace
//     ╭─[ 0:70:1 ]
//     │
//  70 │ }
//     │ ┬  
//     │ ╰── Expected top-level declaration, found RBrace
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Duplicate test 'OuterPyramid'
//    ╭─[ 0:1:1 ]
//    │
//  1 │ function MakePoem(length: int) -> string {
//    │ │ 
//    │ ╰─ test 'OuterPyramid' defined in expr_expr_full.baml
//    │ │ 
//    │ ╰─ 'OuterPyramid' previously defined in expr_expr_full.baml
//    │ 
//    │ Note: Error code: E0011
// ───╯
// Error: Unknown variable poem
//     ╭─[ 0:41:16 ]
//     │
//  41 │   CombinePoems(poem, another)
//     │                ──┬─  
//     │                  ╰─── Unknown variable poem
//     │ 
//     │ Note: Error code: E0003
// ────╯
// Error: Unknown variable another
//     ╭─[ 0:41:22 ]
//     │
//  41 │   CombinePoems(poem, another)
//     │                      ───┬───  
//     │                         ╰───── Unknown variable another
//     │ 
//     │ Note: Error code: E0003
// ────╯
