// End-to-end tests for B-634.
//
// A generic `match` arm `let s: Opt<T>` (where `T` is the enclosing function's
// type var) used to lower its runtime type-test to an EXACT reified type-arg
// comparison. When inference pins `T` to a supertype UNION of the scrutinee's
// actual type arg — which happens whenever a `default: T` argument subtypes,
// so `T` reifies to the un-subsumed join `Shape | Sq` while the value is a
// plain `Opt<Shape>` — the exact `Shape | Sq == Shape` check failed and the arm
// silently missed, falling through to the default.
//
// Fixed in `baml_compiler2_mir::lower::lower_pattern_test`: the typevar-carrying
// class-pattern arm now builds its `IsType` template with
// `tir2_to_dispatch_guard_template` (frame `TypeVar` -> `TypeArgRefOrWildcard`,
// subtype-or-wildcard) instead of `ty_to_template` (frame `TypeVar` ->
// `TypeArgRef`, exact). This matches the interface class-dispatch guard path and
// gives the correct covariant "does this value belong to type `Opt<T>`"
// semantics.
//
// NOTE: test bodies live in top-level helper functions — locals holding class
// instances misbehave when compared/passed inside `test { ... }` blocks (the
// same VM local-boxing caveat documented in `ns_class_type_args_at_runtime`).

interface Shape {
    function area(self) -> int throws never
}

class Sq {
    s: int,
    implements Shape {
        function area(self) -> int {
            self.s * self.s
        }
    }
}

class Opt<T> {
    value: T,
}

class Non {}

function mk<T>(v: T) -> Opt<T> | Non {
    Opt<T> { value: v }
}

function unwrap_or<T>(o: Opt<T> | Non, default: T) -> T {
    match (o) {
        let s: Opt<T> => s.value,
        _ => default,
    }
}

// ─── 1. Narrow default: T infers to the un-subsumed join `Shape | Sq` ─────────
// The scrutinee is a plain `Opt<Shape>`; before the fix the reified frame `T`
// (`Shape | Sq`) failed the exact type-arg check and the arm returned the
// default `Sq { s: 0 }` (area 0). With subtype-or-wildcard, `Shape <: Shape|Sq`
// matches, so we get the wrapped `Sq { s: 5 }` back (area 25).
function generic_match_subtype_narrow_default_fn() -> int {
    let o: Opt<Shape> | Non = mk<Shape>(Sq { s: 5 });
    unwrap_or(o, Sq { s: 0 }).area()
}

test "generic_match_subtype_narrow_default" {
    assert.equal(generic_match_subtype_narrow_default_fn(), 25)
}

// ─── 2. Widened default: T pinned to `Shape` directly ─────────────────────────
// This path already worked before the fix (exact `Shape == Shape`); kept as a
// control to guard against regressing it.
function generic_match_widened_default_fn() -> int {
    let o: Opt<Shape> | Non = mk<Shape>(Sq { s: 5 });
    let d: Shape = Sq { s: 0 };
    unwrap_or(o, d).area()
}

test "generic_match_widened_default" {
    assert.equal(generic_match_widened_default_fn(), 25)
}

// ─── 3. The widened arm still discriminates `Opt<T>` from the non-Opt member ──
// The subtype-or-wildcard template must not blindly match every value: a `Non`
// scrutinee must still fall through to the default (`_` arm). Confirms the fix
// does not collapse the class-name discrimination in the pattern.
function generic_match_falls_through_on_non_fn() -> int {
    let o: Opt<Shape> | Non = Non {};
    unwrap_or(o, Sq { s: 7 }).area()
}

test "generic_match_falls_through_on_non" {
    assert.equal(generic_match_falls_through_on_non_fn(), 49)
}

// ─── 4. Concrete parametric arms still discriminate (unchanged path) ──────────
// `let s: Foo<int>` / `let s: Foo<string>` are concrete (no enclosing type var),
// so they take the exact concrete path, untouched by the fix. They must keep
// discriminating `Foo<int>` from `Foo<string>`.
class Foo<T> {
    value: T,
}

function pick_concrete(x: Foo<int> | Foo<string>) -> string {
    match (x) {
        let a: Foo<int> => "int",
        let b: Foo<string> => "string",
    }
}

function concrete_discriminates_int_fn() -> string {
    pick_concrete(Foo<int> { value: 1 })
}

test "generic_match_concrete_discriminates_int" {
    assert.equal(concrete_discriminates_int_fn(), "int")
}

function concrete_discriminates_string_fn() -> string {
    pick_concrete(Foo<string> { value: "x" })
}

test "generic_match_concrete_discriminates_string" {
    assert.equal(concrete_discriminates_string_fn(), "string")
}
