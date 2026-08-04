// tir: fails
//
// (TIR: null-first render + a throws-contract error on next()'s
// effect.)
//
// I5: the spec's Interfaces `first` example verbatim. A bound WITHOUT
// pinned associated types leaves `T.Item` symbolic in the signature,
// the `next()` call types through the bound, the match narrows
// `baml.iter.Done | T.Item`, and `next()`'s `Self.Error` effect
// reduces to `never` through the interface DEFAULT at the bare bound
// reference (spec: omitted defaulted associated types "use said
// defaults") - which is what makes the declared `throws never` sound.
function first<T extends baml.iter.Iterator>(it: T) -> T.Item? throws never {
    let r = match (it.next()) {
        baml.iter.Done => null,
        let item: T.Item => item,
    };
    r
 // ^ (T as baml.iter.Iterator).Item | null
}
