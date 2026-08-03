// pending: I5 projection reduction + stdlib Iterator shapes
//
// tir: fails
//
// (TIR: null-first render + a throws-contract error on next()'s
// effect.)
//
// The spec's Interfaces `first` example verbatim: a bound WITHOUT
// pinned associated types leaves `T.Item` symbolic in the signature,
// the `next()` call types through the bound, and the match narrows
// `baml.iter.Done | T.Item`. Requires projection reduction through
// interface defaults and the Done sentinel handling.
function first<T extends baml.iter.Iterator>(it: T) -> T.Item? throws never {
    let r = match (it.next()) {
        baml.iter.Done => null,
        let item: T.Item => item,
    };
    r
 // ^ (T as baml.iter.Iterator).Item | null
}
