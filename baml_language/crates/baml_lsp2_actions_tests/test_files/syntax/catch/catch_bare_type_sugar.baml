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
// Error: unreachable arm
//     ╭─[ catch_catch_bare_type_sugar.baml:16:21 ]
//     │
//  16 │     TimeoutError => "timeout",
//     │                     ────┬────  
//     │                         ╰────── unreachable arm
//     │ 
//     │ Note: Error code: E0001
// ────╯
// Error: unreachable arm
//     ╭─[ catch_catch_bare_type_sugar.baml:24:21 ]
//     │
//  24 │     TimeoutError => "timeout",
//     │                     ────┬────  
//     │                         ╰────── unreachable arm
//     │ 
//     │ Note: Error code: E0001
// ────╯
// Error: unreachable arm
//     ╭─[ catch_catch_bare_type_sugar.baml:25:19 ]
//     │
//  25 │     ParseError => "parse error",
//     │                   ──────┬──────  
//     │                         ╰──────── unreachable arm
//     │ 
//     │ Note: Error code: E0001
// ────╯
// Error: unreachable arm
//     ╭─[ catch_catch_bare_type_sugar.baml:33:15 ]
//     │
//  33 │     string => "string error",
//     │               ───────┬──────  
//     │                      ╰──────── unreachable arm
//     │ 
//     │ Note: Error code: E0001
// ────╯
