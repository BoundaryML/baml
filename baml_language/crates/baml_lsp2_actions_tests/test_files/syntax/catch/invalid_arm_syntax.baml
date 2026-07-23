function MayFail(x: int) -> string {
  "ok"
}

function InvalidArmSyntax(x: int) -> string {
  MayFail(x) catch (e) {
    string => "ok",
    let y = "hi";
  }
}

//----
//- diagnostics
// E0010
//
//   × unexpected token
//    ╭─[catch_invalid_arm_syntax.baml:8:11]
//  8 │     let y = "hi";
//    ·           ┬
//    ·           ╰── expected `'=>' after catch pattern`, found `'='`
//    ╰────
// E0063
//
//   ⚠ unreachable arm
//    ╭─[catch_invalid_arm_syntax.baml:7:15]
//  7 │     string => "ok",
//    ·               ────
//    ╰────
// E0063
//
//   ⚠ unreachable arm
//    ╭─[catch_invalid_arm_syntax.baml:8:5]
//  8 │     let y = "hi";
//    ·     ─────────────
//    ╰────
