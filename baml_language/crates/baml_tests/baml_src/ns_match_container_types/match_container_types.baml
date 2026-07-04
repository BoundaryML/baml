// End-to-end tests for element-discriminating container type tests in `match`
// and `is`.
//
// Before the MIR/emit structural-routing fix, `int[]` and `string[]` both
// lowered their runtime type test to a single coarse `LIST` type tag (element
// type discarded), so a `string[]` value wrongly matched an `int[]` arm — and a
// `map<string,int>` arm accepted a `map<string,string>` value. The fix routes an
// element-discriminating container template through the VM's structural value
// matcher (`ConstValue::Type`), which relates generic-argument positions
// invariantly: `int[]` is not `string[]`, `map<string,int>` is not
// `map<string,string>`.
//
// Locals holding container values are fine inside functions; test bodies call
// top-level helpers (the VM local-boxing caveat only bites class instances).

// ─── 1. list element discrimination via `match` (2-arm chain) ─────────────────

function classify_list(x: int[] | string[]) -> string {
    match (x) {
        let a: int[] => "ints",
        let b: string[] => "strings",
    }
}

function classify_int_list_fn() -> string {
    classify_list([1, 2, 3])
}

test "match_container_int_list" {
    assert.equal(classify_int_list_fn(), "ints")
}

function classify_string_list_fn() -> string {
    classify_list(["a", "b"])
}

test "match_container_string_list" {
    assert.equal(classify_string_list_fn(), "strings")
}

// ─── 2. list element discrimination via `is` ──────────────────────────────────

function int_list_is_int_array_fn() -> bool {
    let x: int[] | string[] = [1, 2, 3];
    x is int[]
}

test "is_int_list_is_int_array" {
    assert.is_true(int_list_is_int_array_fn())
}

function string_list_is_int_array_fn() -> bool {
    let x: int[] | string[] = ["a", "b"];
    x is int[]
}

test "is_string_list_is_not_int_array" {
    assert.is_true(baml.deep_equals(string_list_is_int_array_fn(), false))
}

// ─── 3. map value-type discrimination via `match` ─────────────────────────────

function classify_map(x: map<string, int> | map<string, string>) -> string {
    match (x) {
        let a: map<string, int> => "int values",
        let b: map<string, string> => "string values",
    }
}

function classify_int_map_fn() -> string {
    let m: map<string, int> = { "k": 1 };
    classify_map(m)
}

test "match_container_int_map" {
    assert.equal(classify_int_map_fn(), "int values")
}

function classify_string_map_fn() -> string {
    let m: map<string, string> = { "k": "v" };
    classify_map(m)
}

test "match_container_string_map" {
    assert.equal(classify_string_map_fn(), "string values")
}
