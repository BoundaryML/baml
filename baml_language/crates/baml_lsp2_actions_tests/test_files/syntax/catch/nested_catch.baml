// Nested catch expressions — inner catch inside outer catch arm
function MayFail(x: int) -> string {
  "ok"
}

function AlsoMayFail(x: int) -> string {
  "also ok"
}

function NestedCatchInArm(x: int) -> string {
  MayFail(x) catch (e) {
    _ => AlsoMayFail(x) catch (e2) {
      _ => "double caught"
    }
  }
}

// Catch result used in let binding
function CatchInLet(x: int) -> string {
  let result = MayFail(x) catch (e) {
    _ => "fallback"
  };
  result
}

//----
//- diagnostics
// E0063
//
//   ⚠ unreachable arm
//     ╭─[catch_nested_catch.baml:12:10]
//  12 │ ╭─▶     _ => AlsoMayFail(x) catch (e2) {
//  13 │ │         _ => "double caught"
//  14 │ ╰─▶     }
//     ╰────
// E0063
//
//   ⚠ unreachable arm
//     ╭─[catch_nested_catch.baml:13:12]
//  13 │       _ => "double caught"
//     ·            ───────────────
//     ╰────
// E0063
//
//   ⚠ unreachable arm
//     ╭─[catch_nested_catch.baml:21:10]
//  21 │     _ => "fallback"
//     ·          ──────────
//     ╰────
