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
// Warning: Unreachable catch arm
//     ╭─[ catch_catch_bare_type_sugar.baml:24:5 ]
//     │
//  24 │     TimeoutError => "timeout",
//     │     ──────┬─────  
//     │           ╰─────── Unreachable catch arm
//     │ 
//     │ Note: Error code: E0095
// ────╯
// Warning: Unreachable catch arm
//     ╭─[ catch_catch_bare_type_sugar.baml:25:5 ]
//     │
//  25 │     ParseError => "parse error",
//     │     ─────┬────  
//     │          ╰────── Unreachable catch arm
//     │ 
//     │ Note: Error code: E0095
// ────╯
// Warning: Unreachable catch arm
//     ╭─[ catch_catch_bare_type_sugar.baml:26:5 ]
//     │
//  26 │     _ => "unknown"
//     │     ┬  
//     │     ╰── Unreachable catch arm
//     │ 
//     │ Note: Error code: E0095
// ────╯
// Warning: Unreachable catch arm
//     ╭─[ catch_catch_bare_type_sugar.baml:33:5 ]
//     │
//  33 │     string => "string error",
//     │     ───┬──  
//     │        ╰──── Unreachable catch arm
//     │ 
//     │ Note: Error code: E0095
// ────╯
// Warning: Unreachable catch arm
//     ╭─[ catch_catch_bare_type_sugar.baml:34:5 ]
//     │
//  34 │     _ => "other"
//     │     ┬  
//     │     ╰── Unreachable catch arm
//     │ 
//     │ Note: Error code: E0095
// ────╯
// Warning: Unreachable catch arm
//     ╭─[ catch_catch_bare_type_sugar.baml:16:5 ]
//     │
//  16 │     TimeoutError => "timeout",
//     │     ──────┬─────  
//     │           ╰─────── Unreachable catch arm
//     │ 
//     │ Note: Error code: E0095
// ────╯
// Warning: Unreachable catch arm
//     ╭─[ catch_catch_bare_type_sugar.baml:17:5 ]
//     │
//  17 │     _ => "other error"
//     │     ┬  
//     │     ╰── Unreachable catch arm
//     │ 
//     │ Note: Error code: E0095
// ────╯
