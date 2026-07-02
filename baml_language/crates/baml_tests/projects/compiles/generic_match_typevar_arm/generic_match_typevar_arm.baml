// B-634: a generic `match` arm `let s: Opt<T>` (where `T` is the enclosing
// function's type var) must lower its runtime type-test through the
// subtype-or-wildcard dispatch-guard template, so the reified frame type-arg is
// compared with `is_subtype_of` rather than exact `==`.
//
// This project locks the emitted template: the `Opt<T>` arm's IsType constant
// must carry `TypeArgRefOrWildcard(0)` (rendered `#0?`), not the exact
// `TypeArgRef(0)` (rendered `#0`), in the *_04_5_mir / *_06_codegen snapshots.

class Opt<T> {
  value: T,
}

class Non {}

function unwrap_or<T>(o: Opt<T> | Non, default: T) -> T {
  match (o) {
    let s: Opt<T> => s.value,
    _ => default,
  }
}
