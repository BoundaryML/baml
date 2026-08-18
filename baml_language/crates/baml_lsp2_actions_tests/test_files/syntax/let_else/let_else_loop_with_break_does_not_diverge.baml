// The mirror of `let_else_diverges_loop`: a statically-`true` loop that
// holds a `break` bound to it DOES have an exit edge, so control falls
// through past the binding and E0113 still applies. A condition that is
// not statically `true` keeps the ordinary zero-iteration rule.
class Ok { value string }
class Err { message string }

function LoopWithBreakFallsThrough(r: Ok | Err) -> string {
  let o: Ok = r else { while (true) { break; } };
  o.value
}

function LoopWithBreakInsideIfFallsThrough(r: Ok | Err, stop: bool) -> string {
  // `break` reached through a nested branch still binds to this loop.
  let o: Ok = r else {
    while (true) {
      if (stop) { break; }
    }
  };
  o.value
}

function BreakInTheUpdateSlotFallsThrough(r: Ok | Err) -> string {
  // The C-style update slot is part of THIS loop, so a `break` there is a
  // real exit edge even though the body holds none.
  let o: Ok = r else {
    for (let i = 0; 1 == 1; i = break) {}
  };
  o.value
}

function NonLiteralConditionFallsThrough(r: Ok | Err, keep: bool) -> string {
  let o: Ok = r else { while (keep) {} };
  o.value
}

//----
//- diagnostics
// E0113
//
//   × `let … else` requires a diverging else block (`return`, `throw`, `break`, or `continue`); got `void`
//    ╭─[let_else_let_else_loop_with_break_does_not_diverge.baml:9:22]
//  9 │   let o: Ok = r else { while (true) { break; } };
//    ·                      ───────────────────────────
//    ╰────
// E0113
//
//   × `let … else` requires a diverging else block (`return`, `throw`, `break`, or `continue`); got `void`
//     ╭─[let_else_let_else_loop_with_break_does_not_diverge.baml:15:22]
//  15 │ ╭─▶   let o: Ok = r else {
//  16 │ │       while (true) {
//  17 │ │         if (stop) { break; }
//  18 │ │       }
//  19 │ ╰─▶   };
//     ╰────
// E0113
//
//   × `let … else` requires a diverging else block (`return`, `throw`, `break`, or `continue`); got `void`
//     ╭─[let_else_let_else_loop_with_break_does_not_diverge.baml:26:22]
//  26 │ ╭─▶   let o: Ok = r else {
//  27 │ │       for (let i = 0; 1 == 1; i = break) {}
//  28 │ ╰─▶   };
//     ╰────
// E0113
//
//   × `let … else` requires a diverging else block (`return`, `throw`, `break`, or `continue`); got `void`
//     ╭─[let_else_let_else_loop_with_break_does_not_diverge.baml:33:22]
//  33 │   let o: Ok = r else { while (keep) {} };
//     ·                      ───────────────────
//     ╰────
