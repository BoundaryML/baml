//
// (TIR's null-first union render: `null | (T as
// baml.iter.Iterator).Item`.)
//
// I5: a bound WITHOUT pinned associated types leaves every projection
// symbolic - INCLUDING defaulted ones (ruled 2026-08-11: defaults
// realize at impl selection only, rustc's model; an impl may override
// a default, so reducing `T.Error` to its default at the bound would
// drop the pinned type). `next()`'s effect is therefore the symbolic
// `(T as baml.iter.Iterator).Error`, and the sound signature
// PROPAGATES it; `T extends baml.iter.Iterator<Error = never>` is the
// pinned spelling when a caller needs `throws never`.
function first<T extends baml.iter.Iterator>(it: T) -> T.Item? throws T.Error {
    let r = match (it.next()) {
        baml.iter.Done => null,
        let item: T.Item => item,
    };
    r
 // ^ (T as baml.iter.Iterator).Item | null
}
