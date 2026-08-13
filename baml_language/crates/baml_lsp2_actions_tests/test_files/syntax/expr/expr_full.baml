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
// E0010
//
//   × Jinja `#"..."#` prompts are no longer supported. Use a backtick prompt with `${...}` interpolation instead.
//    ╭─[expr_expr_full.baml:3:12]
//  3 │     prompt #"Write a poem {{ length }} lines long."#
//    ·            ────────────────────┬────────────────────
//    ·                                ╰── use a backtick prompt instead
//    ╰────
// E0010
//
//   × Jinja `#"..."#` prompts are no longer supported. Use a backtick prompt with `${...}` interpolation instead.
//     ╭─[expr_expr_full.baml:8:12]
//   8 │ ╭─▶     prompt #"Combine the following two poems into one poem.
//   9 │ │
//  10 │ │       Poem 1:
//  11 │ │       {{ poem1 }}
//  12 │ │
//  13 │ │       Poem 2:
//  14 │ │       {{ poem2 }}
//  15 │ ├─▶     "#
//     · ╰──── use a backtick prompt instead
//     ╰────
// E0010
//
//   × `client<llm>` config blocks are removed; declare a client value instead: `client GPT4o = openai.OpenAiClient.new(model = "...");` (compose reliability with ai.Retry / ai.Fallback /
//   │ ai.RoundRobin)
//     ╭─[expr_expr_full.baml:59:1]
//  59 │ ╭─▶ client<llm> GPT4o {
//  60 │ │     provider openai
//  61 │ │     options {
//  62 │ │       model gpt-4o
//  63 │ │       api_key env.OPENAI_API_KEY
//  64 │ │     }
//  65 │ ├─▶ }
//     · ╰──── replace with `client Name = <expr>;`
//     ╰────
// E0003
//
//   × unresolved name: `functions`
//     ╭─[expr_expr_full.baml:45:3]
//  45 │   functions [Pipeline]
//     ·   ─────────
//     ╰────
// E0002
//
//   × unresolved type: args
//     ╭─[expr_expr_full.baml:46:3]
//  46 │   args { }
//     ·   ────────
//     ╰────
// E0003
//
//   × unresolved name: `TestPipeline`
//     ╭─[expr_expr_full.baml:44:6]
//  44 │ test TestPipeline() {
//     ·      ────────────
//     ╰────
// E0003
//
//   × unresolved name: `functions`
//     ╭─[expr_expr_full.baml:50:3]
//  50 │   functions [Pyramid]
//     ·   ─────────
//     ╰────
// E0002
//
//   × unresolved type: args
//     ╭─[expr_expr_full.baml:51:3]
//  51 │   args { }
//     ·   ────────
//     ╰────
// E0003
//
//   × unresolved name: `TestPyramid`
//     ╭─[expr_expr_full.baml:49:6]
//  49 │ test TestPyramid() {
//     ·      ───────────
//     ╰────
// E0003
//
//   × unresolved name: `functions`
//     ╭─[expr_expr_full.baml:55:3]
//  55 │   functions [OuterPyramid]
//     ·   ─────────
//     ╰────
// E0002
//
//   × unresolved type: args
//     ╭─[expr_expr_full.baml:56:3]
//  56 │   args { }
//     ·   ────────
//     ╰────
// E0003
//
//   × unresolved name: `functions`
//     ╭─[expr_expr_full.baml:68:5]
//  68 │     functions [MakePoem]
//     ·     ─────────
//     ╰────
// E0003
//
//   × unresolved name: `args`
//     ╭─[expr_expr_full.baml:69:5]
//  69 │     args { length 4 }
//     ·     ────
//     ╰────
// E0003
//
//   × unresolved name: `length`
//     ╭─[expr_expr_full.baml:69:12]
//  69 │     args { length 4 }
//     ·            ──────
//     ╰────
// E0003
//
//   × unresolved name: `TestMakePoem`
//     ╭─[expr_expr_full.baml:67:6]
//  67 │ test TestMakePoem() {
//     ·      ────────────
//     ╰────
// E0003
//
//   × unresolved name: `poem`
//     ╭─[expr_expr_full.baml:41:16]
//  41 │   CombinePoems(poem, another)
//     ·                ────
//     ╰────
// E0003
//
//   × unresolved name: `another`
//     ╭─[expr_expr_full.baml:41:22]
//  41 │   CombinePoems(poem, another)
//     ·                      ───────
//     ╰────
