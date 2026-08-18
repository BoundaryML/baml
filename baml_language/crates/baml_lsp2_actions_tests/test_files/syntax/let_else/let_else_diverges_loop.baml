// A `while (true)` whose body holds no `break` bound to it never reaches
// the code after it, so it is a divergence form for `let … else` — the
// same role `loop {}` plays in Rust, where its type is `!`.
class Ok { value string }
class Err { message string }

function LoopDiverges(r: Ok | Err) -> string {
  let o: Ok = r else { while (true) {} };
  o.value
}

function ContinueIsNotAnExit(r: Ok | Err) -> string {
  // `continue` re-enters the loop; only `break` leaves it.
  let o: Ok = r else { while (true) { continue; } };
  o.value
}

function NestedLoopBreakStillDiverges(r: Ok | Err) -> string {
  // The `break` binds to the INNER loop, so the outer loop still has no
  // exit edge and the else branch still diverges.
  let o: Ok = r else {
    while (true) {
      while (true) { break; }
    }
  };
  o.value
}

function FoldedTrueDiverges(r: Ok | Err) -> string {
  // The condition folds to the literal type `true` at the interned layer.
  let o: Ok = r else { while (1 == 1) {} };
  o.value
}

//----
//- diagnostics
// <no-diagnostics-expected>
