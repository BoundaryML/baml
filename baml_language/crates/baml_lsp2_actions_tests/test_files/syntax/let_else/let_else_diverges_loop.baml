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

function NarrowedBindingDiverges(r: Ok | Err, c: bool) -> string {
  // The oracle is the condition's inferred type, so a binding flow-narrowed
  // to `true` counts. This stays sound because a loop havocs every binding
  // its body assigns before the condition is checked — a binding the loop
  // could falsify would no longer be narrowed here.
  // The `if` diverges only when BOTH arms do, so the `while` is what this
  // case actually proves: swap it for `while (keep)` and E0113 returns.
  let o: Ok = r else {
    if (c is true) {
      while (c) {}
    } else {
      throw "not narrowed"
    }
  };
  o.value
}

//----
//- diagnostics
// <no-diagnostics-expected>
