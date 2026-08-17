let G: int | float = 10.0;

function Ok() -> int {
  let x: int | float = 10.0;
  let y : int = 10.0;
  let c: int = "hello";
  1
}

// error: Error validating: Type mismatch: variable 'y' annotated as int but got float
//   -->  expr/let_annotations.baml:5
//    |
//  4 |   let x: int | float = 10.0;
//  5 |   let y : int = 10.0;
//  6 |   let c: int = "hello";
//    |
// error: Error validating: Type mismatch: variable 'c' annotated as int but got string
//   -->  expr/let_annotations.baml:6
//    |
//  5 |   let y : int = 10.0;
//  6 |   let c: int = "hello";
//  7 |   1
//    |

//----
//- diagnostics
// E0010
//
//   × top-level `let` bindings are not supported
//    ╭─[expr_let_annotations.baml:1:1]
//  1 │ let G: int | float = 10.0;
//    · ─────────────┬────────────
//    ·              ╰── move this binding into a function body
//    ╰────
// E0001
//
//   × mismatched types
//    ╭─[expr_let_annotations.baml:5:17]
//  5 │   let y : int = 10.0;
//    ·                 ──┬─
//    ·                   ╰── expected `int`, found `10.0`
//    ╰────
// E0001
//
//   × mismatched types
//    ╭─[expr_let_annotations.baml:6:16]
//  6 │   let c: int = "hello";
//    ·                ───┬───
//    ·                   ╰── expected `int`, found `"hello"`
//    ╰────
