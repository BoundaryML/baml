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
    string => "handled string",
    let other => throw other
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
// warning: unreachable arm
//    ╭─[ throw_throw_in_catch.baml:9:9 ]
//    │
//  9 │     _ => throw e
//    │         ────┬───  
//    │             ╰───── unreachable arm
//    │ 
//    │ Note: Error code: E0063
// ───╯
// warning: unreachable arm
//     ╭─[ throw_throw_in_catch.baml:16:15 ]
//     │
//  16 │     string => "handled string",
//     │               ────────┬───────  
//     │                       ╰───────── unreachable arm
//     │ 
//     │ Note: Error code: E0063
// ────╯
// warning: unreachable arm
//     ╭─[ throw_throw_in_catch.baml:17:17 ]
//     │
//  17 │     let other => throw other
//     │                 ──────┬─────  
//     │                       ╰─────── unreachable arm
//     │ 
//     │ Note: Error code: E0063
// ────╯
// warning: unreachable arm
//     ╭─[ throw_throw_in_catch.baml:24:9 ]
//     │
//  24 │     _ => throw "wrapped: original error"
//     │         ────────────────┬───────────────  
//     │                         ╰───────────────── unreachable arm
//     │ 
//     │ Note: Error code: E0063
// ────╯
