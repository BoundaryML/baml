// A diverging let-else branch cannot contribute flow facts to the success
// continuation. Assigning a literal inside the else narrows that path only;
// the post-let use must retain the local's declared union type.
class Ok { value string }
class Err { message string }

function PreserveOuterFlow(r: Ok | Err, input: int | string) -> int | string {
  let value: int | string = input;
  let o: Ok = r else {
    value = "else-only";
    return 0;
  };
  value<[CURSOR]
}

//----
//- diagnostics
// <no-diagnostics-expected>
//
//- on_hover expressions
// hover at let_else_let_else_else_flow_does_not_leak.baml:13:8
// ```baml
// value: int | string
// ```
