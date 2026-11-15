// Behaviour the heap object model must keep. Each test pins what the language
// does today, so a change of representation that alters one of these is a
// language change, not a refactor. Every helper collects garbage between
// building a value and observing it, so the observation also covers a move.
//
// Bodies live in top-level functions rather than test blocks (see
// ns_lambdas/lambdas.baml for why).

enum PinColor {
    Red,
    Green,
}

class PinBox {
    items: int[],
}

class PinMixed {
    name: string,
    count: int,
    ratio: float,
    ok: bool,
    tags: string[],
}

class PinCounter {
    n: int,

    function add(self, k: int) -> int {
        self.n + k
    }
}

function pin_add_one(x: int) -> int {
    x + 1
}

// ── deep_copy: what gets a new identity ─────────────────────────────────────

// A copied function value is equal to the original and still runs.
function pin_function_copy() -> bool[] {
    let f = pin_add_one;
    let g = baml.deep_copy(f);
    baml.sys.collect_garbage();
    [f == g, g(1) == 2]
}

test "deep_copy of a function is equal to it and still runs" {
    assert.equal(pin_function_copy(), [true, true])
}

// Closures compare by reference too; the copy shares the captured variable.
function pin_closure_copy() -> int[] {
    let x = 10;
    let bump = () -> int { x += 1; x };
    let copy = baml.deep_copy(bump);
    baml.sys.collect_garbage();
    let same = if (copy == bump) { 1 } else { 0 };
    bump();
    copy();
    [same, x]
}

test "deep_copy of a closure is a new closure over the same variables" {
    assert.equal(pin_closure_copy(), [0, 12])
}

// A bound method compares by function and receiver; the copy keeps both.
function pin_bound_method_copy() -> int[] {
    let c = PinCounter { n: 5 };
    let m = c.add;
    let copy = baml.deep_copy(m);
    baml.sys.collect_garbage();
    c.n = 7;
    let same = if (copy == m) { 1 } else { 0 };
    [same, copy(1)]
}

test "deep_copy of a bound method keeps its receiver" {
    assert.equal(pin_bound_method_copy(), [1, 8])
}

function pin_variant_copy() -> bool[] {
    let red = PinColor.Red;
    let copy = baml.deep_copy(red);
    baml.sys.collect_garbage();
    [copy == red, copy == PinColor.Green]
}

test "deep_copy of an enum value is equal to it" {
    assert.equal(pin_variant_copy(), [true, false])
}

// ── deep_copy: sharing and independence ─────────────────────────────────────

// Two references to one box stay one box in the copy, apart from the original.
function pin_copy_keeps_sharing() -> int[] {
    let shared = PinBox { items: [1] };
    let pair = [shared, shared];
    let copy = baml.deep_copy(pair);
    baml.sys.collect_garbage();
    copy[0].items.push(2);
    [copy[1].items.length(), pair[0].items.length()]
}

test "deep_copy keeps shared references shared" {
    assert.equal(pin_copy_keeps_sharing(), [2, 1])
}

function pin_copy_mixed_fields() -> string {
    let m = PinMixed { name: "a", count: 3, ratio: 0.5, ok: true, tags: ["x"] };
    let copy = baml.deep_copy(m);
    m.tags.push("y");
    m.ratio = 2.5;
    baml.sys.collect_garbage();
    baml.json.stringify(copy)
}

test "deep_copy of an instance keeps field order and values" {
    assert.equal(pin_copy_mixed_fields(), `{"name":"a","count":3,"ratio":0.5,"ok":true,"tags":["x"]}`)
}

// ── maps keep insertion order through deletes, copies and collections ───────

function pin_map_order() -> string[][] {
    let m: map<string, int> = {};
    m["c"] = 1;
    m["a"] = 2;
    m["b"] = 3;
    m.delete("a");
    m["a"] = 4;
    let copy = baml.deep_copy(m);
    baml.sys.collect_garbage();
    [m.keys(), copy.keys()]
}

test "map key order survives delete, re-insert, deep_copy and gc" {
    assert.equal(pin_map_order(), [["c", "b", "a"], ["c", "b", "a"]])
}

// ── floats ──────────────────────────────────────────────────────────────────

function pin_float_values() -> bool[] {
    let nan = float.nan();
    let values: float[] = [0.1, 0.2, nan];
    let held: map<string, float> = { "x": 1.5 };
    baml.sys.collect_garbage();
    [values[0] + values[1] == 0.30000000000000004, values[2] == values[2], nan == nan, held["x"] == 1.5]
}

test "floats keep their value and equality through collections" {
    assert.equal(pin_float_values(), [true, true, true, true])
}

function pin_float_captured() -> float {
    let total = 0.25;
    let add = (d: float) -> float { total += d; total };
    add(0.5);
    baml.sys.collect_garbage();
    add(1.0)
}

test "a captured float variable keeps its value" {
    assert.equal(pin_float_captured(), 1.75)
}
