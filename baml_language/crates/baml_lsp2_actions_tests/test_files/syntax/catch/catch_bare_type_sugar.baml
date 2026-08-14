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
// <no-diagnostics-expected>
