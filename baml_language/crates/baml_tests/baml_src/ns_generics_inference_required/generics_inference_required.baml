// The determining sources for a generic parameter's instantiation slot. Each
// of these compiles and runs; the slot left with none of them is E0002
// (`projects/diagnostic_errors/uninferred_type_param`).

function phantom<T>() -> int {
    0
}

class Picker {
    function pick<U>(self) -> int {
        0
    }
}

class Tag<T> {
    name string
}

function consume<T>(x: T) -> int {
    0
}

function make<T>() -> T? {
    null
}

test "a turbofish determines a phantom parameter" {
    assert.equal(phantom<int>(), 0)
}

test "a turbofish determines a phantom method parameter" {
    let p = Picker {};
    assert.equal(p.pick<int>(), 0)
}

test "an annotation determines a phantom class parameter" {
    let t: Tag<int> = Tag { name: "x" };
    assert.equal(t.name, "x")
}

test "an argument determines a parameter" {
    assert.equal(consume(1), 0)
}

test "the expected type determines a return-only parameter" {
    let v: int? = make();
    assert.is_true(v == null)
}

// A generic that occurs only in `throws` positions is an effect: with no
// evidence it is the empty effect `never`, exactly as the elided spelling
// (`cb: (() -> int)? = null`) gives.
function run_optional<E>(cb: (() -> int throws E)? = null) -> int throws E {
    if (cb != null) {
        cb()
    } else {
        2
    }
}

function raise_nothing<E>() -> int throws E {
    3
}

test "an effect-only generic with no evidence is never" {
    assert.equal(run_optional(), 2)
    assert.equal(run_optional(cb = null), 2)
    assert.equal(raise_nothing(), 3)
    assert.is_true(ai.events.guard(null) == null)
}

function pick<T>(a: T, b: T) -> T {
    a
}

test "the expected type decides between conflicting arguments" {
    let r: int | string = pick(1, "s");
    assert.equal(r, 1)
}

// What the expected type decides is a real solution: every goal that waited
// on it runs against it, so a use the solution admits goes through.
function apply_both<T>(a: T, b: T, f: (T) -> T throws never) -> T throws never {
    f(b)
}

test "a goal waiting on a decided class runs against the decision" {
    let r: int | string = apply_both(1, "s", (x) -> { x });
    assert.equal(r, "s")
}

// A bounded parameter in `throws` positions is not an effect: evidence
// decides it, and its bound is checked against what it decides.
interface GirMaker {
    function make() -> int throws never
}

class GirOne {
    implements GirMaker {
        function make() -> int throws never { 1 }
    }
}

function make_through<E extends GirMaker>(cb: (() -> int throws E)? = null) -> int throws E {
    if (cb != null) { cb() } else { E.make() }
}

test "evidence decides a bounded throws-position parameter" {
    assert.equal(make_through<GirOne>(), 1)
}

// An invariant argument is equated where it relates: the call below solves
// `O` to the union at once, so the `match` sees a known scrutinee.
class GirSpec<T> {
    v T
}

class GirRes<T> {
    value T
}

function gir_run<O>(s: GirSpec<O>) -> GirRes<O> {
    GirRes { value: s.v }
}

function gir_match_union() -> int {
    let s: GirSpec<int | string> = GirSpec { v: 3 };
    let r = gir_run(s).value;
    match (r) {
        let i: int => i,
        let t: string => 0,
    }
}

test "a match sees a union an invariant argument solved at the call" {
    assert.equal(gir_match_union(), 3)
}
