function Foo1(arg: int) -> float {
  client Foo
}

function Foo2(arg: int) -> float {
  prompt `...`
}

function Foo3(arg: int) -> float {
  client bar
  prompt "..."
}


function Foo4(arg: int) -> float {
  client bar
  prompt
}


function Foo5(arg: int) -> float {
  client
  prompt `...`
}


function Foo6(arg: int) -> float {
  client
  prompt
}

//----
//- diagnostics
// E0010
//
//   × unexpected token
//    ╭─[functions_v2_invalid2.baml:3:1]
//  3 │ }
//    · ┬
//    · ╰── expected `LLM function missing 'prompt' field`, found `'}'`
//    ╰────
// E0010
//
//   × unexpected token
//    ╭─[functions_v2_invalid2.baml:7:1]
//  7 │ }
//    · ┬
//    · ╰── expected `LLM function missing 'client' field`, found `'}'`
//    ╰────
// E0010
//
//   × unexpected token
//     ╭─[functions_v2_invalid2.baml:18:1]
//  18 │ }
//     · ┬
//     · ╰── expected `prompt string`, found `'}'`
//     ╰────
// E0010
//
//   × unexpected token
//     ╭─[functions_v2_invalid2.baml:23:3]
//  23 │   prompt `...`
//     ·   ───┬──
//     ·      ╰── expected `client value`, found `identifier`
//     ╰────
// E0010
//
//   × unexpected token
//     ╭─[functions_v2_invalid2.baml:29:3]
//  29 │   prompt
//     ·   ───┬──
//     ·      ╰── expected `client value`, found `identifier`
//     ╰────
// E0010
//
//   × unexpected token
//     ╭─[functions_v2_invalid2.baml:30:1]
//  30 │ }
//     · ┬
//     · ╰── expected `prompt string`, found `'}'`
//     ╰────
