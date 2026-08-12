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
// E0063
//
//   ⚠ unreachable arm
//     ╭─[catch_catch_binding_narrowing.baml:16:21]
//  16 │     TimeoutError => "retry",
//     ·                     ───────
//     ╰────
// E0063
//
//   ⚠ unreachable arm
//     ╭─[catch_catch_binding_narrowing.baml:17:10]
//  17 │     _ => "other"
//     ·          ───────
//     ╰────
// E0063
//
//   ⚠ unreachable arm
//     ╭─[catch_catch_binding_narrowing.baml:24:21]
//  24 │     TimeoutError => "timeout",
//     ·                     ─────────
//     ╰────
// E0063
//
//   ⚠ unreachable arm
//     ╭─[catch_catch_binding_narrowing.baml:25:19]
//  25 │     ParseError => "parse error",
//     ·                   ─────────────
//     ╰────
// E0063
//
//   ⚠ unreachable arm
//     ╭─[catch_catch_binding_narrowing.baml:26:10]
//  26 │     _ => "unknown"
//     ·          ─────────
//     ╰────
