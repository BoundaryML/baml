// I6: a stdlib DEFAULT method (`collect`, a real body on
// `baml.iter.Iterator`) called through a bare bound: the signature
// realizes at `Self` = `T`, `Self.Item[]` becomes the symbolic
// projection's array, and `throws Self.Error` stays the symbolic
// `(T as baml.iter.Iterator).Error` (ruled 2026-08-11: defaults
// realize at impl selection only, never at a bound reference), so the
// sound signature propagates it.
function drain<T extends baml.iter.Iterator>(it: T) -> T.Item[] throws T.Error {
    let r = it.collect();
     // ^ (T as baml.iter.Iterator).Item[]
    r
}
