// BEP-049 §7 + §11: backtick interpolation is strict-typed. Values
// interpolated with `${...}` are wrapped with `.to_string()` at lowering.
// A *nullable* value has no direct `to_string` (it may be null), so the
// compiler errors at type-check time rather than silently rendering "null" /
// "<undefined>" / some other invented form.
//
// (A class is NOT an error: `to_string` is auto-derived on every class — like
// `to_json` — so any class instance is interpolatable. Positive runtime
// coverage lives in bex_engine/tests/backtick_strings.rs.)
//
// Quote from BEP §7 (rationale for strict-bool truthiness): "It also
// surfaces null-handling bugs in prompts at type-check time instead of
// at LLM-call time." Same logic applies to null interpolation: the error
// fires before the LLM is ever called.
//
// This fixture pins the diagnostic wording for the nullable cases:
//   (A) null literal interpolation
//   (E) optional (nullable) primitive interpolation

// (A) Interpolating a `null` literal.
function case_a_null_interp() -> string {
    let x: int? = null
    `val: ${x}`
}

// (E) Interpolating an optional/nullable primitive. Same root cause as (A):
// `int?` has no direct `to_string`; the user must coalesce or unwrap first.
function get_count() -> int? {
    42
}

function case_e_optional_int_interp() -> string {
    let c = get_count()
    `count: ${c}`
}
