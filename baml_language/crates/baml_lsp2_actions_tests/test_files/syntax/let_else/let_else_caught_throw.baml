// A throw from a let-else else block must participate in the throws
// contract and be catchable at the call site. BAML's catch syntax is
// `expr catch (e) { Pattern => arm, ... }` — a match-like dispatch on
// the caught error.
class Ok { value string }
class Err { message string }
class NoMatch {}

function inner(r: Ok | Err) -> string throws NoMatch {
  let o: Ok = r else { throw NoMatch {}; };
  o.value
}

// The caller doesn't declare `throws NoMatch` — it catches the throw,
// so the throws set is empty here.
function caller(r: Ok | Err) -> string {
  inner(r) catch (e) {
    NoMatch => "caught"
  }
}

//----
//- diagnostics
// <no-diagnostics-expected>
