// Multi-segment receiver with type-args on a static method call.
//
// Generic class `Box<T>` lives in nested namespace `pkg.inner`.
// `root.pkg.inner.Box<int>.from_json(j)` resolves through cross-namespace
// path-based lookup AND threads the receiver's type-arg `<int>` into the
// user-defined `Box.from_json` frame — the static-method counterpart to
// the instance-method type-arg threading exercised by
// `class_type_args_at_runtime.rs`.
//
// Compiles-tier fixture: zero diagnostics + bytecode snapshot prove the
// resolution worked.  Regression test for the `Expr::MemberAccess`
// `base_is_value` heuristic in `builder.rs`: a multi-segment `Path` whose
// root is not a local variable must be treated as a type name (unbound
// static method), not as a value (bound instance method) — otherwise the
// receiver's class-level type-args are misinterpreted as method-level
// type-args of the static callee, producing
// "function `from_json` expects 0 type argument(s), got 1".
function decode_root(j: baml.json.json) -> root.pkg.inner.Box<int> throws baml.json.JsonParseError | baml.json.JsonDecodeError {
    root.pkg.inner.Box<int>.from_json(j)
}

// Auto-derived counterpart: `AutoBox<T>` has no user `from_json`, so this
// call must resolve to the synthesized `AutoBox.from_json` and still thread
// the receiver's `<int>` through the synthesizer's frame.
function decode_auto(j: baml.json.json) -> root.pkg.inner.AutoBox<int> throws baml.json.JsonParseError | baml.json.JsonDecodeError {
    root.pkg.inner.AutoBox<int>.from_json(j)
}
