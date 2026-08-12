//
// B-633 (the live half): https://linear.app/boundaryml2/issue/B-633
// A rigid `T` arm does not cover the `null` member of a `T | null`
// scrutinee - claiming a union member requires PROVABLE overlap, never
// TIR's TypeVar-blanket-true. The match is non-exhaustive (Error until
// S17 renders E0062 with the `null` witness); TIR accepts it silently
// and lets null bind to a non-nullable `T` at runtime.
class Holder<T> {
    items T[]
}

function bad_pop<T>(h: Holder<T>) -> T {
    let r = match (h.items.pop()) { let x: T => x };
     // ^ !error
  //        ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^ non-exhaustive
    r
}
