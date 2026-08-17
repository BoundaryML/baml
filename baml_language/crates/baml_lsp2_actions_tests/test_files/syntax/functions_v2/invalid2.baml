function Foo1(arg: int) -> float {
  client: Foo
}

function Foo2(arg: int) -> float {
  prompt: `...`
}

function Foo3(arg: int) -> float {
  client: bar
  prompt: "..."
}


function Foo4(arg: int) -> float {
  client: bar
  prompt: }


function Foo5(arg: int) -> float {
  client:   prompt: `...`
}


function Foo6(arg: int) -> float {
  client:   prompt: }

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
//     ╭─[functions_v2_invalid2.baml:17:11]
//  17 │   prompt: }
//     ·           ┬
//     ·           ╰── expected `prompt string`, found `'}'`
//     ╰────
// E0010
//
//   × unexpected token
//     ╭─[functions_v2_invalid2.baml:21:19]
//  21 │   client:   prompt: `...`
//     ·                   ┬
//     ·                   ╰── expected `Only 'client', 'tools' and 'prompt' allowed in LLM function, found ':'`, found `':'`
//     ╰────
// E0010
//
//   × unexpected token
//     ╭─[functions_v2_invalid2.baml:21:21]
//  21 │   client:   prompt: `...`
//     ·                     ┬
//     ·                     ╰── expected `Only 'client', 'tools' and 'prompt' allowed in LLM function, found '`'`, found `'`'`
//     ╰────
// E0010
//
//   × unexpected token
//     ╭─[functions_v2_invalid2.baml:21:22]
//  21 │   client:   prompt: `...`
//     ·                      ─┬─
//     ·                       ╰── expected `Only 'client', 'tools' and 'prompt' allowed in LLM function, found '...'`, found `'...'`
//     ╰────
// E0010
//
//   × unexpected token
//     ╭─[functions_v2_invalid2.baml:21:25]
//  21 │   client:   prompt: `...`
//     ·                         ┬
//     ·                         ╰── expected `Only 'client', 'tools' and 'prompt' allowed in LLM function, found '`'`, found `'`'`
//     ╰────
// E0010
//
//   × unexpected token
//     ╭─[functions_v2_invalid2.baml:22:1]
//  22 │ }
//     · ┬
//     · ╰── expected `LLM function missing 'prompt' field`, found `'}'`
//     ╰────
// E0010
//
//   × unexpected token
//     ╭─[functions_v2_invalid2.baml:26:19]
//  26 │   client:   prompt: }
//     ·                   ┬
//     ·                   ╰── expected `Only 'client', 'tools' and 'prompt' allowed in LLM function, found ':'`, found `':'`
//     ╰────
// E0010
//
//   × unexpected token
//     ╭─[functions_v2_invalid2.baml:26:21]
//  26 │   client:   prompt: }
//     ·                     ┬
//     ·                     ╰── expected `LLM function missing 'prompt' field`, found `'}'`
//     ╰────
// E0003
//
//   × unresolved name: `bar`
//     ╭─[functions_v2_invalid2.baml:10:11]
//  10 │   client: bar
//     ·           ───
//     ╰────
