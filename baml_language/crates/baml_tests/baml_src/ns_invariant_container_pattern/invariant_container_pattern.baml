// Type patterns over invariant containers (E0176).
//
// Generics are invariant (TYPE_SYSTEM.md), and arrays/maps carry their element
// type at runtime, so a `let a: unknown[]` arm matches only arrays built at
// element type exactly `unknown`. An `int[]` value passed as `unknown` falls
// through it. Matching it would be unsound: the arm binds `a: unknown[]` to the
// same `int[]` object, so `a.push("x")` would put a string into an `int[]`.
//
// The runtime semantics are pinned below; the compiler warns (E0176) wherever
// such a pattern meets a scrutinee that can hold a differently-typed container
// (`unknown`, a type variable, an alias like `json`), and stays quiet where the
// test is exact. A concrete scrutinee like `int[] | string` already rejects an
// `unknown[]` arm outright (E0001). The warning sites are in the snapshot
// `diagnostics.snap`.

// --- runtime semantics ---------------------------------------------------------

// E0176 on both container arms: `v: unknown` can hold any array or map.
function classify(v: unknown) -> string {
    match (v) {
        let m: map<string, unknown> => "map"
        let a: unknown[] => "array"
        _ => "other"
    }
}

test "unknown_array_arm_misses_int_array" {
    let xs: int[] = [1, 2];
    assert.equal(classify(xs), "other")
}

test "unknown_map_arm_misses_int_map" {
    let m: map<string, int> = {"a": 1};
    assert.equal(classify(m), "other")
}

test "unknown_array_arm_matches_unknown_array" {
    let xs: unknown[] = [1, 2];
    assert.equal(classify(xs), "array")
}

test "unknown_map_arm_matches_unknown_map" {
    let m: map<string, unknown> = {"a": 1};
    assert.equal(classify(m), "map")
}

// The workaround the warning points at: reflect the runtime element type, then
// re-match against it.
function array_length(v: unknown) -> int? {
    let at = reflect.Type.of_value(v).as_array();
    if (at == null) {
        return null;
    }
    type E = unreflect(at.element_type());
    match (v) {
        let xs: E[] => xs.length()
        _ => null
    }
}

test "reflect_workaround_sees_int_array" {
    let xs: int[] = [1, 2, 3];
    assert.equal(array_length(xs), 3)
}

test "reflect_workaround_sees_string_array" {
    let xs: string[] = ["a"];
    assert.equal(array_length(xs), 1)
}

test "reflect_workaround_rejects_non_array" {
    assert.equal(array_length("abc"), null)
}

// --- E0176 fires -----------------------------------------------------------------

// `is` is the same runtime test as a match arm.
function is_unknown_array(v: unknown) -> bool {
    v is unknown[]
}

test "is_unknown_array_misses_int_array" {
    let xs: int[] = [1];
    assert.equal(is_unknown_array(xs), false)
}

// As a match arm this is a hard E0001 (the arm can never match); `is` makes
// the same dead test legal and silently `false`, so it gets E0176 instead.
function is_int_array_or_string_unknown_array(v: int[] | string) -> bool {
    v is unknown[]
}

test "is_on_concrete_scrutinee_is_false" {
    assert.equal(is_int_array_or_string_unknown_array([1]), false)
}

// A nested `unknown` is invariant all the same.
function nested(v: unknown) -> bool {
    v is map<string, unknown[]>
}

test "nested_unknown_misses_typed_value" {
    let m: map<string, int[]> = {"a": [1]};
    assert.equal(nested(m), false)
}

// A type variable can be instantiated at any container.
function generic_is_unknown_array<T>(v: T) -> bool {
    v is unknown[]
}

test "generic_scrutinee_misses_int_array" {
    let xs: int[] = [1];
    assert.equal(generic_is_unknown_array(xs), false)
}

// `json` holds `json[]` arrays, which are not `unknown[]`.
function json_is_unknown_array(v: json) -> bool {
    v is unknown[]
}

test "json_array_misses_unknown_array" {
    let v: json = [1, 2];
    assert.equal(json_is_unknown_array(v), false)
}

// Futures are invariant containers too.
function one() -> int { 1 }

function is_unknown_future(v: unknown) -> bool {
    v is baml.future.Future<unknown, never>
}

test "int_future_misses_unknown_future" {
    let f = spawn { one() };
    assert.equal(is_unknown_future(f), false)
}

// --- E0176 stays quiet -----------------------------------------------------------

// The scrutinee's container member is exactly the pattern: the test is exact.
function exact_array_or_null(v: unknown[] | null) -> string {
    match (v) {
        let a: unknown[] => "array"
        null => "null"
    }
}

// Both instantiations listed: the arm deliberately picks one of them.
function unknown_or_int_array(v: unknown[] | int[]) -> string {
    match (v) {
        let a: unknown[] => "unknowns"
        let i: int[] => "ints"
    }
}

function exact_map_or_int(v: map<string, unknown> | int) -> string {
    match (v) {
        let m: map<string, unknown> => "map"
        _ => "int"
    }
}

test "exact_scrutinee_matches" {
    let xs: unknown[] = [1];
    let m: map<string, unknown> = {"a": 1};
    assert.equal(exact_array_or_null(xs), "array");
    assert.equal(exact_map_or_int(m), "map");
    assert.equal(exact_map_or_int(1), "int");
    let ints: int[] = [1];
    assert.equal(unknown_or_int_array(xs), "unknowns");
    assert.equal(unknown_or_int_array(ints), "ints")
}

// A pattern that does not mention `unknown` says exactly what it means.
function is_int_array(v: unknown) -> bool {
    v is int[]
}

test "concrete_container_pattern_matches" {
    let xs: int[] = [1];
    assert.is_true(is_int_array(xs))
}
