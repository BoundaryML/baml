// No throws clause — function body throws, no diagnostic

function NoThrowsAnnotation(x: int) -> string {
  if (x < 0) {
    throw "error"
  }
  "ok"
}

//----
//- diagnostics
// <no-diagnostics-expected>
