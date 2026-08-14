// `break` is `Ty::Never` only when there's a containing loop; inside a
// `while` body it counts as divergence. Type-checks without errors.
class Ok { value string }
class Err { message string }

function BreakOutOfLoopOnMiss(items: (Ok | Err)[]) -> string {
  let result = "";
  let i = 0;
  while (i < 3) {
    let item = items[i];
    let o: Ok = item else { break; };
    result = o.value;
    i += 1;
  }
  result
}

//----
//- diagnostics
// <no-diagnostics-expected>
