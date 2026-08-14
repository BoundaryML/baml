// `void` return — only legal in bare return-type position. Codegen lowers
// to `Unit`, which Python renders as `None`. Any expression body works as
// long as it type-checks; we use a no-op for the simplest possible body.

function no_op() -> void {
}
