// catch with only wildcard arm — unreachable because MayFail never throws
function MayFail(x: int) -> string {
  "ok"
}

function CatchWildcardOnly(x: int) -> string {
  MayFail(x) catch (e) {
    _ => "caught everything"
  }
}

// Wildcard with binding name
function CatchNamedWildcard(x: int) -> string {
  MayFail(x) catch (e) {
    let other => "caught: " + other
  }
}

//----
//- diagnostics
// <no-diagnostics-expected>
