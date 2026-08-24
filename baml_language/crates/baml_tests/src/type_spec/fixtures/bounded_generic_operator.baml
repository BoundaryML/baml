//
// The spec's Interfaces example verbatim: `T extends baml.ops.Add<O>`
// lets `lhs + rhs` dispatch through the CARRIED bound (I2's param
// env), typing as the symbolic projection - the canonical render of
// the signature's own `T.Output`. Reduction of the projection at
// realized instantiations joins with I5; TIR renders `unknown` here.
function add<O, T extends baml.ops.Add<O>>(lhs: T, rhs: O) -> T.Output throws never {
    lhs + rhs
 // ^^^^^^^^^ (T as baml.ops.Add<O>).Output
}
