// catch attached to a non-callable expression should error
function CatchOnLiteral(x: int) -> string {
  "hello" catch (e) {
    _ => "caught"
  }
}

function CatchOnVariable(x: int) -> string {
  x catch (e) {
    _ => "caught"
  }
}

//----
//- diagnostics
// E0001
//
//   × mismatched types
//     ╭─[catch_catch_on_non_call.baml:8:44]
//   8 │ ╭─▶ function CatchOnVariable(x: int) -> string {
//   9 │ │     x catch (e) {
//  10 │ │       _ => "caught"
//  11 │ │     }
//  12 │ ├─▶ }
//     · ╰──── expected `string`, found `int | "caught"`
//     ╰────
