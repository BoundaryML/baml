// catch_all (exhaustive): partial handling is an error

function MayFailIntOrString(x: int) -> string {
    match (x) {
        0 => throw "string error",
        1 => throw 1,
        _ => "ok"
    }
}

// catch_all with only string arm — int is unhandled → E0062
function PartialCatchAll(x: int) -> string {
  MayFailIntOrString(x) catch_all (e) {
    string => "caught string"
  }
}

//----
//- diagnostics
// <no-diagnostics-expected>
