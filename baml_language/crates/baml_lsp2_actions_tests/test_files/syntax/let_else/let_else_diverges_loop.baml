// Known limitation: BAML's TIR doesn't infer `while (true) { … }` as
// `Ty::Never` (it sees it as a regular `while` with type `void`), so
// using an infinite loop as a divergence form in let-else is rejected.
// This is a TIR-wide gap, not specific to let-else — same applies to
// `if` chains in tail position. Tracked as a follow-up.
//
// In Rust the equivalent `let X = e else { loop {} };` is accepted
// because `loop {}` has type `!`.
class Ok { value string }
class Err { message string }

function LoopDiverges(r: Ok | Err) -> string {
  let o: Ok = r else { while (true) {} };
  o.value
}

//----
//- diagnostics
// <no-diagnostics-expected>
