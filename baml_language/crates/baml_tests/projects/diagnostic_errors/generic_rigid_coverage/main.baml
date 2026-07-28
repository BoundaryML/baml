// Coverage semantics for rigid-typevar arms (the diagnostics snapshot is the
// assertion):
//
// - A rigid arm is a possible-but-not-covering row against concrete scrutinee
//   members, so a match where it is the only arm is NON-exhaustive.
// - Against its own type (canonical equality), a rigid arm covers reflexively,
//   so a `_` after it is unreachable.

// `T` can match either member at some realization, but *covers* neither — the
// match must not be considered exhaustive.
function rigid_only_arm_is_not_exhaustive<T>(x: int | string) -> string {
    match (x) {
        let t: T => "T",
    }
}

// Reflexivity: over a scrutinee of exactly `T`, the `let t: T` arm is a
// definite cover, so the trailing `_` can never be reached.
function wildcard_after_reflexive_arm_is_unreachable<T>(x: T) -> string {
    match (x) {
        let t: T => "T",
        _ => "other",
    }
}

// The mirror direction: a CONCRETE pattern over a rigid scrutinee matches only
// the realizations that make `T` its subtype — it covers nothing, so a match
// with only concrete arms over `T` is non-exhaustive. (This used to compile
// "exhaustive" and skip the arm's runtime test, binding a foreign
// realization's value at type `string`.)
function concrete_only_arm_over_rigid_is_not_exhaustive<T>(x: T) -> string {
    match (x) {
        let s: string => s,
    }
}
