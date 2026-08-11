function MakePoem(length: int) -> string {
    client: GPT4o
    prompt: `Write a poem ${length} lines long.`
}

function CombinePoems(poem1: string, poem2: string) -> string {
    client: GPT4o
    prompt: `
        Combine the following two poems into one poem.

            Poem 1:
            ${poem1}

            Poem 2:
            ${poem2}
    `
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
    let xy = CombinePoems(x, y);
    let ab = CombinePoems(a, b);
    CombinePoems(xy, ab)
}

function Pyramid() -> string {
    CombinePoems(CombinePoems(MakePoem(10), MakePoem(10)), MakePoem(10))
}

function OuterPyramid() -> string {
    CombinePoems(poem, another)
}

test TestPipeline() {
    functions[Pipeline];
    args {  }
}

test TestPyramid() {
    functions[Pyramid];
    args {  }
}

test OuterPyramid() {
    functions[OuterPyramid];
    args {  }
}

client GPT4o = openai.OpenAiClient.new(
    model = "gpt-4o",
    api_key = env.OPENAI_API_KEY,
);

test TestMakePoem() {
    functions[MakePoem];
    args;
    {
        length;
        4
    }
}

//----
//- diagnostics
// E0003
//
//   × unresolved name: `poem`
//     ╭─[expr_expr_full.baml:42:18]
//  42 │     CombinePoems(poem, another)
//     ·                  ────
//     ╰────
// E0003
//
//   × unresolved name: `another`
//     ╭─[expr_expr_full.baml:42:24]
//  42 │     CombinePoems(poem, another)
//     ·                        ───────
//     ╰────
// E0003
//
//   × unresolved name: `TestPipeline`
//     ╭─[expr_expr_full.baml:45:6]
//  45 │ test TestPipeline() {
//     ·      ────────────
//     ╰────
// E0003
//
//   × unresolved name: `functions`
//     ╭─[expr_expr_full.baml:46:5]
//  46 │     functions[Pipeline];
//     ·     ─────────
//     ╰────
// E0002
//
//   × unresolved type: args
//     ╭─[expr_expr_full.baml:47:5]
//  47 │     args {  }
//     ·     ─────────
//     ╰────
// E0003
//
//   × unresolved name: `TestPyramid`
//     ╭─[expr_expr_full.baml:50:6]
//  50 │ test TestPyramid() {
//     ·      ───────────
//     ╰────
// E0003
//
//   × unresolved name: `functions`
//     ╭─[expr_expr_full.baml:51:5]
//  51 │     functions[Pyramid];
//     ·     ─────────
//     ╰────
// E0002
//
//   × unresolved type: args
//     ╭─[expr_expr_full.baml:52:5]
//  52 │     args {  }
//     ·     ─────────
//     ╰────
// E0003
//
//   × unresolved name: `functions`
//     ╭─[expr_expr_full.baml:56:5]
//  56 │     functions[OuterPyramid];
//     ·     ─────────
//     ╰────
// E0002
//
//   × unresolved type: args
//     ╭─[expr_expr_full.baml:57:5]
//  57 │     args {  }
//     ·     ─────────
//     ╰────
// E0003
//
//   × unresolved name: `TestMakePoem`
//     ╭─[expr_expr_full.baml:65:6]
//  65 │ test TestMakePoem() {
//     ·      ────────────
//     ╰────
// E0003
//
//   × unresolved name: `functions`
//     ╭─[expr_expr_full.baml:66:5]
//  66 │     functions[MakePoem];
//     ·     ─────────
//     ╰────
// E0003
//
//   × unresolved name: `args`
//     ╭─[expr_expr_full.baml:67:5]
//  67 │     args;
//     ·     ────
//     ╰────
// E0003
//
//   × unresolved name: `length`
//     ╭─[expr_expr_full.baml:69:9]
//  69 │         length;
//     ·         ──────
//     ╰────
// E0003
//
//   × unresolved name: `functions`
//     ╭─[expr_expr_full.baml:46:5]
//  46 │     functions[Pipeline];
//     ·     ─────────
//     ╰────
// E0002
//
//   × unresolved type: args
//     ╭─[expr_expr_full.baml:47:5]
//  47 │     args {  }
//     ·     ─────────
//     ╰────
// E0003
//
//   × unresolved name: `functions`
//     ╭─[expr_expr_full.baml:51:5]
//  51 │     functions[Pyramid];
//     ·     ─────────
//     ╰────
// E0002
//
//   × unresolved type: args
//     ╭─[expr_expr_full.baml:52:5]
//  52 │     args {  }
//     ·     ─────────
//     ╰────
// E0003
//
//   × unresolved name: `functions`
//     ╭─[expr_expr_full.baml:56:5]
//  56 │     functions[OuterPyramid];
//     ·     ─────────
//     ╰────
// E0002
//
//   × unresolved type: args
//     ╭─[expr_expr_full.baml:57:5]
//  57 │     args {  }
//     ·     ─────────
//     ╰────
// E0003
//
//   × unresolved name: `functions`
//     ╭─[expr_expr_full.baml:66:5]
//  66 │     functions[MakePoem];
//     ·     ─────────
//     ╰────
// E0003
//
//   × unresolved name: `args`
//     ╭─[expr_expr_full.baml:67:5]
//  67 │     args;
//     ·     ────
//     ╰────
// E0003
//
//   × unresolved name: `length`
//     ╭─[expr_expr_full.baml:69:9]
//  69 │         length;
//     ·         ──────
//     ╰────
