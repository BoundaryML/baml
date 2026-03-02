// Catch binding narrowing: the catch binding variable is narrowed per arm
class TimeoutError {
  retryAfterMs int
}
class ParseError {
  message string
}

function MayFail(x: int) -> string {
  "ok"
}

// Bare type pattern narrows the catch binding
function NarrowCatchBinding(x: int) -> string {
  MayFail(x) catch (e) {
    TimeoutError => "retry",
    _ => "other"
  }
}

// Multiple typed arms each narrow the binding independently
function MultipleNarrowedArms(x: int) -> string {
  MayFail(x) catch (e) {
    TimeoutError => "timeout",
    ParseError => "parse error",
    _ => "unknown"
  }
}

//----
//- diagnostics
// Warning: Unreachable catch arm
//     ╭─[ catch_catch_binding_narrowing.baml:24:5 ]
//     │
//  24 │     TimeoutError => "timeout",
//     │     ──────┬─────  
//     │           ╰─────── Unreachable catch arm
//     │ 
//     │ Note: Error code: E0095
// ────╯
// Warning: Unreachable catch arm
//     ╭─[ catch_catch_binding_narrowing.baml:25:5 ]
//     │
//  25 │     ParseError => "parse error",
//     │     ─────┬────  
//     │          ╰────── Unreachable catch arm
//     │ 
//     │ Note: Error code: E0095
// ────╯
// Warning: Unreachable catch arm
//     ╭─[ catch_catch_binding_narrowing.baml:26:5 ]
//     │
//  26 │     _ => "unknown"
//     │     ┬  
//     │     ╰── Unreachable catch arm
//     │ 
//     │ Note: Error code: E0095
// ────╯
// Warning: Unreachable catch arm
//     ╭─[ catch_catch_binding_narrowing.baml:16:5 ]
//     │
//  16 │     TimeoutError => "retry",
//     │     ──────┬─────  
//     │           ╰─────── Unreachable catch arm
//     │ 
//     │ Note: Error code: E0095
// ────╯
// Warning: Unreachable catch arm
//     ╭─[ catch_catch_binding_narrowing.baml:17:5 ]
//     │
//  17 │     _ => "other"
//     │     ┬  
//     │     ╰── Unreachable catch arm
//     │ 
//     │ Note: Error code: E0095
// ────╯
