//
// I2: a rigid `T extends I` proves `T <: I` through its CARRIED bound
// (type_var_bound in the shared algebra), so the join of a bounded var
// with the interface existential absorbs to the interface. (TIR's
// dedup-only join keeps both.)
function f<T extends baml.ops.Equals>(x: T, e: baml.ops.Equals, c: bool) -> baml.ops.Equals {
    let u = if (c) { x } else { e };
     // ^ baml.ops.Equals
    u
}
