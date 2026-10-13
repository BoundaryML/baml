// `void` return: the unit type, also spelled `null`. Codegen gives a unit
// return the host's own, which Python renders as `None`. A unit body may
// leave out its final expression; a no-op is the simplest possible body.

function no_op() -> void {
}
