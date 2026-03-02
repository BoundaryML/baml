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
// Warning: Unreachable catch arm
//     ╭─[ catch_nested_catch.baml:21:7 ]
//     │
//  21 │     _ => "fallback"
//     │       ┬  
//     │       ╰── Unreachable catch arm
//     │ 
//     │ Note: Error code: E0095
// ────╯
// Warning: Unreachable catch arm
//     ╭─[ catch_nested_catch.baml:12:7 ]
//     │
//  12 │     _ => AlsoMayFail(x) catch (e2) {
//     │       ┬  
//     │       ╰── Unreachable catch arm
//     │ 
//     │ Note: Error code: E0095
// ────╯
// Warning: Unreachable catch arm
//     ╭─[ catch_nested_catch.baml:13:9 ]
//     │
//  13 │       _ => "double caught"
//     │         ┬  
//     │         ╰── Unreachable catch arm
//     │ 
//     │ Note: Error code: E0095
// ────╯
