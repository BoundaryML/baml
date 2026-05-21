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
// warning: unreachable arm
//     ╭─[ catch_nested_catch.baml:12:9 ]
//     │
//  12 │ ╭─▶     _ => AlsoMayFail(x) catch (e2) {
//     ┆ ┆   
//  14 │ ├─▶     }
//     │ │           
//     │ ╰─────────── unreachable arm
//     │     
//     │     Note: Error code: E0063
// ────╯
// warning: unreachable arm
//     ╭─[ catch_nested_catch.baml:13:12 ]
//     │
//  13 │       _ => "double caught"
//     │            ───────┬───────  
//     │                   ╰───────── unreachable arm
//     │ 
//     │ Note: Error code: E0063
// ────╯
// warning: unreachable arm
//     ╭─[ catch_nested_catch.baml:21:10 ]
//     │
//  21 │     _ => "fallback"
//     │          ─────┬────  
//     │               ╰────── unreachable arm
//     │ 
//     │ Note: Error code: E0063
// ────╯
