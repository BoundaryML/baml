// catch_all with wildcard arm — covers everything, no error

function MayFail(x: int) -> string {
    match (x) {
        0 => throw "error",
        _ => "ok"
    }
}

function CatchAllWildcard(x: int) -> string {
  MayFail(x) catch_all (e) {
    _ => "caught everything"
  }
}

//----
//- diagnostics
// <no-diagnostics-expected>
