// I3: Self-typed parameters are SOUND on a rigid bounded receiver (the
// receiver is one concrete type, so `other: Self` pins to it) - `lt`
// comes from the bound directly, `eq` through its requires closure.
function f<T extends baml.ops.Compare>(x: T, y: T) -> bool {
    x.lt(y)
 // ^^^^^^^ bool
}

function g<T extends baml.ops.Compare>(x: T, y: T) -> bool {
    x.eq(y)
 // ^^^^^^^ bool
}
