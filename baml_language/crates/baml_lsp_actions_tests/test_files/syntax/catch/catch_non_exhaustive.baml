// catch (non-exhaustive): partial handling is OK, residual propagates silently

function MayFailIntOrString(x: int) -> string {
    match (x) {
        0 => throw "string error",
        1 => throw 1,
        _ => "ok"
    }
}

// Only catch string; int propagates to caller — no error
function PartialCatch(x: int) -> string {
  MayFailIntOrString(x) catch (e) {
    _: string => "caught string"
  }
}

// catch with no wildcard, only one typed arm — still OK
function SingleTypedArm(x: int) -> string {
  MayFailIntOrString(x) catch (e) {
    _: int => "caught int"
  }
}

//----
//- diagnostics
// <no-diagnostics-expected>
