// pending: I2 param env + I5 projections - the spec's own bounded-add
//
// tir: fails
//
// TYPE_SYSTEM.md's Interfaces example verbatim: `T extends
// baml.ops.Add<O>` must let `lhs + rhs` dispatch through the CARRIED
// bound (type_var_bound) and type as the projection `T.Output` (which
// the signature also names). Until I2 fills the bound oracle and I5
// lowers projections, both sentinel.
function add<O, T extends baml.ops.Add<O>>(lhs: T, rhs: O) -> T.Output throws never {
    lhs + rhs
 // ^^^^^^^^^ T.Output
}
