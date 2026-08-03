// pending: I3 interface member resolution
//
// A method call through an interface existential (virtual dispatch):
// `t.to_string()` on a `baml.ToString` receiver must resolve the
// interface's method (Self-free, so the one-Self rule admits it).
// Method lookup currently only knows classes and builtin receivers.
function f(t: baml.ToString) -> string {
    t.to_string()
 // ^^^^^^^^^^^^^ string
}
