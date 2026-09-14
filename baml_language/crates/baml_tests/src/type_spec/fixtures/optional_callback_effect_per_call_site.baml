//
// Effect polymorphism through an OPTIONAL callback: `?` is a call-site
// shape, not a nesting that hides the slot, so `apply_optional` carries the
// same implicit effect generic as the immediate form
// (`effect_polymorphism_per_call_site.baml`) and instantiates it per call
// site. A non-throwing callback and an absent one both leave the caller at
// `never`; a throwing one moves its exact effect into the caller.
function apply_optional(f: ((x: int) -> int)?, v: int) -> int {
      // ^^^^^^^^^^^^^^ throws __effect_param_0
    if (f != null) {
        return f(v)
    }
    v
}

function safe(v: int) -> int {
      // ^^^^ throws never
    apply_optional((x) -> { x + 1 }, v)
}

function absent(v: int) -> int {
      // ^^^^^^ throws never
    apply_optional(null, v)
}

function risky(v: int) -> int {
      // ^^^^^ throws "neg"
    apply_optional((x) -> { if (x < 0) { throw "neg"; } x }, v)
}
