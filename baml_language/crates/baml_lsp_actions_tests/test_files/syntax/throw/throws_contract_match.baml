// throws contract: declared matches inferred — no diagnostic

function ThrowsStringMatch(x: int) -> string throws string {
  if (x < 0) {
    throw "error"
  }
  "ok"
}

function ThrowsUnionMatch(x: int) -> string throws string | int {
  match (x) {
    0 => throw "error",
    1 => throw 42,
    _ => "ok"
  }
}

//----
//- diagnostics
// <no-diagnostics-expected>
