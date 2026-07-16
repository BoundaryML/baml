// End-to-end tests for B-634.
//
// A generic `match` arm `let s: Opt<T>` (where `T` is the enclosing function's
// type var) compares the scrutinee instance's reified type arg against the
// frame's `T`. When inference pins `T` to a supertype UNION of the scrutinee's
// actual arg — which happens whenever a `default: T` argument subtypes, so `T`
// reifies to the un-subsumed join `Shape | Sq` while the value is a plain
// `Opt<Shape>` — the arm must still match, because `Shape | Sq` *is* `Shape`:
// `Sq` implements `Shape` (`Sq <: Shape`), so the union absorbs to `Shape`.
//
// The original miss came from comparing with the context-free structural
// relation, which has no interface-membership knowledge: it could not absorb
// `Shape | Sq -> Shape`, so the two spellings looked like different types and
// the arm fell through to the default. The runtime now relates class type args
// *invariantly* through the canonical algebra, which knows `Sq <: Shape` and
// absorbs the union, so `equivalent(Shape, Shape | Sq)` holds and the arm
// matches — no covariance, no widening. The arm lowers `T` to a plain
// `TypeArgRef` frame slot; the covariant `TypeArgRefOrWildcard` band-aid it once
// used is gone.
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
// default `Sq { s: 0 }` (area 0). Now the type arg is related invariantly by the
// canonical algebra, which absorbs `Shape | Sq == Shape` (`Sq <: Shape`), so the
// value's `Shape` arg matches `T` and we get the wrapped `Sq { s: 5 }` (area 25).
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
// The invariant type-arg match must not blindly match every value: a `Non`
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

// ─── 5. Concrete parametric arms on the TypeTag switch path ───────────────────
// The same discrimination as §4, but with enough tag-eligible arms that the
// lowering prefers the TypeTag jump-table switch (4+ *distinct* tags —
// colliding tags dedup before the count, so this needs five arms). A class's
// type tag ignores its type args, so before the switch tag-sufficiency gate
// `Foo<int>` and `Foo<string>` shared one tag: the jump table kept only the
// first `Foo` arm and every `Foo` value jumped there. The gate detects the
// collision and falls back to the (arg-precise) chain.

function pick_concrete_switch(x: Foo<int> | Foo<string> | int | string | bool) -> string {
    match (x) {
        let a: Foo<int> => "int",
        let b: Foo<string> => "string",
        let n: int => "prim int",
        let s: string => "prim string",
        let f: bool => "prim bool",
    }
}

function concrete_switch_int_fn() -> string {
    pick_concrete_switch(Foo<int> { value: 1 })
}

test "generic_match_switch_discriminates_int" {
    assert.equal(concrete_switch_int_fn(), "int")
}

function concrete_switch_string_fn() -> string {
    pick_concrete_switch(Foo<string> { value: "x" })
}

test "generic_match_switch_discriminates_string" {
    assert.equal(concrete_switch_string_fn(), "string")
}

function concrete_switch_prim_fn() -> string {
    pick_concrete_switch("hello")
}

test "generic_match_switch_prims_still_dispatch" {
    assert.equal(concrete_switch_prim_fn(), "prim string")
}
