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
// error: unresolved name: poem
//     ╭─[ expr_expr_full.baml:41:16 ]
//     │
//  41 │   CombinePoems(poem, another)
//     │                ──┬─  
//     │                  ╰─── unresolved name: poem
//     │ 
//     │ Note: Error code: E0003
// ────╯
// error: unresolved name: another
//     ╭─[ expr_expr_full.baml:41:22 ]
//     │
//  41 │   CombinePoems(poem, another)
//     │                      ───┬───  
//     │                         ╰───── unresolved name: another
//     │ 
//     │ Note: Error code: E0003
// ────╯
// error: unresolved name: TestPipeline
//     ╭─[ expr_expr_full.baml:44:6 ]
//     │
//  44 │ test TestPipeline() {
//     │      ──────┬─────  
//     │            ╰─────── unresolved name: TestPipeline
//     │ 
//     │ Note: Error code: E0003
// ────╯
// error: unresolved name: functions
//    ╭─[ expr_expr_full.baml:1:1 ]
//    │
//  1 │ function MakePoem(length: int) -> string {
//    │ │ 
//    │ ╰─ unresolved name: functions
//    │ 
//    │ Note: Error code: E0003
// ───╯
// error: unresolved name: TestPyramid
//     ╭─[ expr_expr_full.baml:49:6 ]
//     │
//  49 │ test TestPyramid() {
//     │      ─────┬─────  
//     │           ╰─────── unresolved name: TestPyramid
//     │ 
//     │ Note: Error code: E0003
// ────╯
// error: unresolved name: functions
//    ╭─[ expr_expr_full.baml:1:1 ]
//    │
//  1 │ function MakePoem(length: int) -> string {
//    │ │ 
//    │ ╰─ unresolved name: functions
//    │ 
//    │ Note: Error code: E0003
// ───╯
// error: unresolved name: TestMakePoem
//     ╭─[ expr_expr_full.baml:67:6 ]
//     │
//  67 │ test TestMakePoem() {
//     │      ──────┬─────  
//     │            ╰─────── unresolved name: TestMakePoem
//     │ 
//     │ Note: Error code: E0003
// ────╯
// error: unresolved name: functions
//    ╭─[ expr_expr_full.baml:1:1 ]
//    │
//  1 │ function MakePoem(length: int) -> string {
//    │ │ 
//    │ ╰─ unresolved name: functions
//    │ 
//    │ Note: Error code: E0003
// ───╯
// error: unresolved name: args
//    ╭─[ expr_expr_full.baml:1:3 ]
//    │
//  1 │ function MakePoem(length: int) -> string {
//    │   ┬  
//    │   ╰── unresolved name: args
//    │ 
//    │ Note: Error code: E0003
// ───╯
// error: unresolved name: length
//    ╭─[ expr_expr_full.baml:1:1 ]
//    │
//  1 │ function MakePoem(length: int) -> string {
//    │ │ 
//    │ ╰─ unresolved name: length
//    │ 
//    │ Note: Error code: E0003
// ───╯
// error: unresolved name: functions
//     ╭─[ expr_expr_full.baml:45:3 ]
//     │
//  45 │   functions [Pipeline]
//     │   ────┬────  
//     │       ╰────── unresolved name: functions
//     │ 
//     │ Note: Error code: E0003
// ────╯
// error: unresolved name: functions
//     ╭─[ expr_expr_full.baml:50:3 ]
//     │
//  50 │   functions [Pyramid]
//     │   ────┬────  
//     │       ╰────── unresolved name: functions
//     │ 
//     │ Note: Error code: E0003
// ────╯
// error: unresolved name: functions
//     ╭─[ expr_expr_full.baml:55:3 ]
//     │
//  55 │   functions [OuterPyramid]
//     │   ────┬────  
//     │       ╰────── unresolved name: functions
//     │ 
//     │ Note: Error code: E0003
// ────╯
// error: unresolved name: functions
//     ╭─[ expr_expr_full.baml:68:5 ]
//     │
//  68 │     functions [MakePoem]
//     │     ────┬────  
//     │         ╰────── unresolved name: functions
//     │ 
//     │ Note: Error code: E0003
// ────╯
// error: unresolved name: args
//     ╭─[ expr_expr_full.baml:69:5 ]
//     │
//  69 │     args { length 4 }
//     │     ──┬─  
//     │       ╰─── unresolved name: args
//     │ 
//     │ Note: Error code: E0003
// ────╯
// error: unresolved name: length
//     ╭─[ expr_expr_full.baml:69:12 ]
//     │
//  69 │     args { length 4 }
//     │            ───┬──  
//     │               ╰──── unresolved name: length
//     │ 
//     │ Note: Error code: E0003
// ────╯
