// throws never with catch_all covering everything — no diagnostic

function MayFail(x: int) -> string {
  match (x) {
    0 => throw "error",
    _ => "ok"
  }
}

function ThrowsNeverSafe(x: int) -> string throws never {
  MayFail(x) catch_all (e) {
    _ => "caught"
  }
}

//----
//- diagnostics
// <no-diagnostics-expected>
