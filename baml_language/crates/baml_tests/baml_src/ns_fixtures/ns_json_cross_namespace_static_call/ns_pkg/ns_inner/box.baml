// Generic class `Box<T>` in nested namespace `pkg.inner`, referenced from
// the root namespace via `root.fixtures.json_cross_namespace_static_call.pkg.inner.Box<...>`. `Box<int>.from_json(j)`
// desugars to `baml.json.to<Box<int>>(j)` (the `<int>` parses as the call's
// type args and is applied to the receiver class).
class Box<T> {
    value T
}

// Same shape as `Box<T>`; both exercise the generic `MemberAccess`-form
// receiver of the `from_json` sugar, threading the receiver's `<int>`.
class AutoBox<T> {
    value T
}

// Non-generic class decoded via the qualified PATH form
// `root.fixtures.json_cross_namespace_static_call.pkg.inner.Plain.from_json(j)` — with no type-args this parses as a flat
// `Expr::Path`, exercising the Path branch of the sugar's receiver-type lookup
// (distinct from the generic `MemberAccess` form above).
class Plain {
    n int
}
