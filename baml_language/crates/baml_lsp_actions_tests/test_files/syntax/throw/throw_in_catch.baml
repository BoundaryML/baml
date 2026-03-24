// Throw inside catch handler — rethrow pattern
function MayFail(x: int) -> string {
  "ok"
}

// Rethrow caught error
function RethrowError(x: int) -> string {
  MayFail(x) catch (e) {
    _ => throw e
  }
}

// Conditional rethrow
function ConditionalRethrow(x: int) -> string {
  MayFail(x) catch (e) {
    _: string => "handled string",
    other => throw other
  }
}

// Throw new error inside catch
function ThrowNewInCatch(x: int) -> string {
  MayFail(x) catch (e) {
    _ => throw "wrapped: original error"
  }
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
//     ╭─[ throw_throw_in_catch.baml:9:11 ]
//     │
//   9 │ ╭─▶     _ => throw e
//  10 │ ├─▶   }
//     │ │         
//     │ ╰───────── unreachable arm
//     │     
//     │     Note: Error code: E0001
// ────╯
// Error: unreachable arm
//     ╭─[ throw_throw_in_catch.baml:16:20 ]
//     │
//  16 │     _: string => "handled string",
//     │                    ────────┬───────  
//     │                            ╰───────── unreachable arm
//     │ 
//     │ Note: Error code: E0001
// ────╯
// Error: unreachable arm
//     ╭─[ throw_throw_in_catch.baml:17:15 ]
//     │
//  17 │ ╭─▶     other => throw other
//  18 │ ├─▶   }
//     │ │         
//     │ ╰───────── unreachable arm
//     │     
//     │     Note: Error code: E0001
// ────╯
// Error: unreachable arm
//     ╭─[ throw_throw_in_catch.baml:24:11 ]
//     │
//  24 │ ╭─▶     _ => throw "wrapped: original error"
//  25 │ ├─▶   }
//     │ │         
//     │ ╰───────── unreachable arm
//     │     
//     │     Note: Error code: E0001
// ────╯
