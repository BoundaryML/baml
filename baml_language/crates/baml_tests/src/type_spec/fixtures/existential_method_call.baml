// I3: a method call through an interface existential dispatches
// virtually - `to_string` is Self-free, so the one-Self rule admits
// it, and the default method's own signature types the call.
function f(t: baml.ToString) -> string {
    t.to_string()
 // ^^^^^^^^^^^^^ string
}
