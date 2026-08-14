// catch attached to a non-callable expression should error
function CatchOnLiteral(x: int) -> string {
  "hello" catch (e) {
    _ => "caught"
  }
}

function CatchOnVariable(x: int) -> string {
  x catch (e) {
    _ => "caught"
  }
}

//----
//- diagnostics
// <no-diagnostics-expected>
