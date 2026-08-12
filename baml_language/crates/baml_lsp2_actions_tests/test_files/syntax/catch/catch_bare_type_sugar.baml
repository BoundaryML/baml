// Bare type names in catch arm patterns
class TimeoutError {
  retryAfterMs int
}
class ParseError {
  message string
}

function MayFail(x: int) -> string {
  "ok"
}

// Bare type pattern in catch arm
function CatchBareType(x: int) -> string {
  MayFail(x) catch (e) {
    TimeoutError => "timeout",
    _ => "other error"
  }
}

// Multiple bare type patterns
function CatchMultipleBareTypes(x: int) -> string {
  MayFail(x) catch (e) {
    TimeoutError => "timeout",
    ParseError => "parse error",
    _ => "unknown"
  }
}

// Bare primitive type in catch
function CatchBarePrimitive(x: int) -> string {
  MayFail(x) catch (e) {
    string => "string error",
    _ => "other"
  }
}

//----
//- diagnostics
// E0063
//
//   ⚠ unreachable arm
//     ╭─[catch_catch_bare_type_sugar.baml:16:21]
//  16 │     TimeoutError => "timeout",
//     ·                     ─────────
//     ╰────
// E0063
//
//   ⚠ unreachable arm
//     ╭─[catch_catch_bare_type_sugar.baml:17:10]
//  17 │     _ => "other error"
//     ·          ─────────────
//     ╰────
// E0063
//
//   ⚠ unreachable arm
//     ╭─[catch_catch_bare_type_sugar.baml:24:21]
//  24 │     TimeoutError => "timeout",
//     ·                     ─────────
//     ╰────
// E0063
//
//   ⚠ unreachable arm
//     ╭─[catch_catch_bare_type_sugar.baml:25:19]
//  25 │     ParseError => "parse error",
//     ·                   ─────────────
//     ╰────
// E0063
//
//   ⚠ unreachable arm
//     ╭─[catch_catch_bare_type_sugar.baml:26:10]
//  26 │     _ => "unknown"
//     ·          ─────────
//     ╰────
// E0063
//
//   ⚠ unreachable arm
//     ╭─[catch_catch_bare_type_sugar.baml:33:15]
//  33 │     string => "string error",
//     ·               ──────────────
//     ╰────
// E0063
//
//   ⚠ unreachable arm
//     ╭─[catch_catch_bare_type_sugar.baml:34:10]
//  34 │     _ => "other"
//     ·          ───────
//     ╰────
