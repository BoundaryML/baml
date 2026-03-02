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
// Warning: Unreachable catch arm
//     ╭─[ throw_throw_in_catch.baml:24:7 ]
//     │
//  24 │     _ => throw "wrapped: original error"
//     │       ┬  
//     │       ╰── Unreachable catch arm
//     │ 
//     │ Note: Error code: E0095
// ────╯
// Warning: Unreachable catch arm
//    ╭─[ throw_throw_in_catch.baml:9:7 ]
//    │
//  9 │     _ => throw e
//    │       ┬  
//    │       ╰── Unreachable catch arm
//    │ 
//    │ Note: Error code: E0095
// ───╯
// Warning: Unreachable catch arm
//     ╭─[ throw_throw_in_catch.baml:16:7 ]
//     │
//  16 │     _: string => "handled string",
//     │       ────┬────  
//     │           ╰────── Unreachable catch arm
//     │ 
//     │ Note: Error code: E0095
// ────╯
// Warning: Unreachable catch arm
//     ╭─[ throw_throw_in_catch.baml:17:7 ]
//     │
//  17 │     other => throw other
//     │       ──┬──  
//     │         ╰──── Unreachable catch arm
//     │ 
//     │ Note: Error code: E0095
// ────╯
