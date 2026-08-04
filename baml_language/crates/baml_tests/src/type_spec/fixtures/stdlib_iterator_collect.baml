// tir: fails
//
// (TIR does not reduce `Self.Error` through the interface default at
// the bare bound, so it reports a throws-contract violation against
// the declared `never` - the iterator_projection_example divergence.)
//
// I6: a stdlib DEFAULT method (`collect`, a real body on
// `baml.iter.Iterator`) called through a bare bound: the signature
// realizes at `Self` = `T`, `Self.Item[]` becomes the symbolic
// projection's array, and `throws Self.Error` reduces to `never`
// through the interface default at the bare bound reference - so the
// declared `throws never` here verifies.
function drain<T extends baml.iter.Iterator>(it: T) -> T.Item[] throws never {
    let r = it.collect();
     // ^ (T as baml.iter.Iterator).Item[]
    r
}
