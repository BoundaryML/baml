// Generic class `Box<T>` in nested namespace `pkg.inner`, referenced from
// the root namespace via `root.pkg.inner.Box<...>`.  `from_json` is the
// auto-derived `baml.FromJson` delegate, so the multi-segment static call
// `root.pkg.inner.Box<int>.from_json(j)` has something to dispatch to.
class Box<T> {
    value T
}

// Same shape as `Box<T>`; both rely on the auto-derived `from_json` delegate.
// Confirms the cross-namespace + receiver-type-arg fix (`builder.rs`
// `Expr::MemberAccess` `base_is_value` heuristic) flows the receiver's `<T>`
// through the delegate's frame.
class AutoBox<T> {
    value T
}
