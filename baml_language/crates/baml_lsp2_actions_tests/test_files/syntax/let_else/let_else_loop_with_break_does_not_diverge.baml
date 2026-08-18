// The mirror of `let_else_diverges_loop`: a `while (true)` whose body
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
//     ╭─[let_else_let_else_loop_with_break_does_not_diverge.baml:24:22]
//  24 │   let o: Ok = r else { while (keep) {} };
//     ·                      ───────────────────
//     ╰────
