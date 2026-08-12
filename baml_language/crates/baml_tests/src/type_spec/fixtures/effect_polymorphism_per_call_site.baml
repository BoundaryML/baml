//
// (TIR diverges only via thrown-literal widening: `throws string` for
// risky. The polymorphism itself agrees.)
//
// Effect polymorphism, concretely: ONE `apply`, whose throws is the
// implicit effect generic (`throws __effect_param_0` - it throws
// whatever its callback throws), instantiated differently per call
// site. A non-throwing callback leaves the caller at `never`; a
// throwing one moves its exact effect into the caller.
function apply(f: (x: int) -> int, v: int) -> int {
    f(v)
}

function safe(v: int) -> int {
      // ^^^^ throws never
    apply((x) -> { x + 1 }, v)
}

function risky(v: int) -> int {
      // ^^^^^ throws "neg"
    apply((x) -> { if (x < 0) { throw "neg"; } x }, v)
}
