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
// Error: remove parentheses from test name: `test TestPipeline`
//     ╭─[ 0:44:18 ]
//     │
//  44 │ test TestPipeline() {
//     │                  ─┬  
//     │                   ╰── remove parentheses from test name: `test TestPipeline`
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: remove parentheses from test name: `test TestPyramid`
//     ╭─[ 0:49:17 ]
//     │
//  49 │ test TestPyramid() {
//     │                 ─┬  
//     │                  ╰── remove parentheses from test name: `test TestPyramid`
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: remove parentheses from test name: `test OuterPyramid`
//     ╭─[ 0:54:18 ]
//     │
//  54 │ test OuterPyramid() {
//     │                  ─┬  
//     │                   ╰── remove parentheses from test name: `test OuterPyramid`
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: remove parentheses from test name: `test TestMakePoem`
//     ╭─[ 0:67:18 ]
//     │
//  67 │ test TestMakePoem() {
//     │                  ─┬  
//     │                   ╰── remove parentheses from test name: `test TestMakePoem`
//     │ 
//     │ Note: Error code: E0010
// ────╯
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
