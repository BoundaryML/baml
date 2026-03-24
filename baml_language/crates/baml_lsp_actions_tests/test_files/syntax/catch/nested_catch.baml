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
// Error: unreachable arm
//      ╭─[ llm_types.baml:159:17 ]
//      │
//  159 │             _ => { continue; }
//      │                 ───────┬──────  
//      │                        ╰──────── unreachable arm
//      │ 
//      │ Note: Error code: E0001
// ─────╯
// Error: unreachable arm
//      ╭─[ llm_types.baml:189:17 ]
//      │
//  189 │ ╭─▶             _ => {
//      ┆ ┆   
//  194 │ ├─▶             }
//      │ │                   
//      │ ╰─────────────────── unreachable arm
//      │     
//      │     Note: Error code: E0001
// ─────╯
// Error: unreachable arm
//      ╭─[ llm_types.baml:208:17 ]
//      │
//  208 │             _ => { continue; }
//      │                 ───────┬──────  
//      │                        ╰──────── unreachable arm
//      │ 
//      │ Note: Error code: E0001
// ─────╯
// Error: unreachable arm
//     ╭─[ catch_nested_catch.baml:12:11 ]
//     │
//  12 │ ╭─▶     _ => AlsoMayFail(x) catch (e2) {
//     ┆ ┆   
//  15 │ ├─▶   }
//     │ │         
//     │ ╰───────── unreachable arm
//     │     
//     │     Note: Error code: E0001
// ────╯
// Error: unreachable arm
//     ╭─[ catch_nested_catch.baml:13:14 ]
//     │
//  13 │ ╭─▶       _ => "double caught"
//  14 │ ├─▶     }
//     │ │           
//     │ ╰─────────── unreachable arm
//     │     
//     │     Note: Error code: E0001
// ────╯
// Error: unreachable arm
//     ╭─[ catch_nested_catch.baml:21:12 ]
//     │
//  21 │ ╭─▶     _ => "fallback"
//  22 │ ├─▶   };
//     │ │          
//     │ ╰────────── unreachable arm
//     │     
//     │     Note: Error code: E0001
// ────╯
