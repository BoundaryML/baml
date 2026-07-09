// End-to-end tests for typevar-carrying `match` arms and `is` tests: bare `T`,
// `T[]`, and `map<string, T>`.
//
// A typevar arm compares the value against the *realized* type bound to the
// typevar in the enclosing call frame (TYPE_SYSTEM.md "Type Variables": at any
// run-time usage site a type variable corresponds to exactly one realized
// type). Before the structural-routing fix, a bare `T` arm lowered to a
// constant-false test (silent arm miss — the value fell through to a later
// arm), and a `T[]` arm lowered to the coarse "any list" tag.
//
// Note the arms are genuinely dynamic: `pick<string>("hi")` takes the `T` arm,
// not the later `string` arm, because with `T = string` the value is a member
// of the first arm's type-set. This is intended set-membership semantics, not
// an ordering bug.
//
// BUG(static-coverage): the unreachable-arm analysis treats a typevar arm as
// covering the whole scrutinee, so an unguarded arm after `let t: T` is
// rejected as unreachable (E0063) — even though at runtime `T` covers only its
// realized type and later arms are reachable (e.g. `pick<int>("hi")` must reach
// the `string` arm). The `if true` guards below exist solely to keep the later
// arms statically reachable; remove them when the coverage analysis models
// typevar arms as covering only their own type-set.

// ─── 1. bare `T` arm (non-last), both polarities ──────────────────────────────

function pick<T>(x: T | string) -> string {
    match (x) {
        let t: T if true => "T",
        let s: string => "string",
        _ => "other",
    }
}

function pick_int_value_fn() -> string {
    pick<int>(42)
}

test "typevar_arm_matches_realized_int" {
    assert.equal(pick_int_value_fn(), "T")
}

function pick_string_value_fn() -> string {
    pick<int>("hi")
}

test "typevar_arm_rejects_non_member" {
    assert.equal(pick_string_value_fn(), "string")
}

function pick_dynamic_capture_fn() -> string {
    pick<string>("hi")
}

test "typevar_arm_dynamic_first_match_capture" {
    assert.equal(pick_dynamic_capture_fn(), "T")
}

// ─── 2. `T[]` arm: element compared against the realized binding ──────────────

// (No trailing wildcard here: the coverage analysis deems the `string[]` arm
// to cover the `T[]` member too, so a wildcard after it is rejected as
// unreachable.)
function pick_list<T>(x: T[] | string[]) -> string {
    match (x) {
        let t: T[] if true => "T[]",
        let s: string[] => "string[]",
    }
}

function pick_list_int_fn() -> string {
    pick_list<int>([1, 2])
}

test "typevar_list_arm_matches_realized_element" {
    assert.equal(pick_list_int_fn(), "T[]")
}

function pick_list_string_fn() -> string {
    pick_list<int>(["a", "b"])
}

test "typevar_list_arm_rejects_other_element" {
    assert.equal(pick_list_string_fn(), "string[]")
}

function pick_list_dynamic_fn() -> string {
    pick_list<string>(["a", "b"])
}

test "typevar_list_arm_dynamic_first_match_capture" {
    assert.equal(pick_list_dynamic_fn(), "T[]")
}

// ─── 3. `map<string, T>` arm ──────────────────────────────────────────────────

// (No trailing wildcard here: the coverage analysis deems the
// `map<string, string>` arm to cover the `map<string, T>` member too, so a
// wildcard after it is rejected as unreachable.)
function pick_map<T>(x: map<string, T> | map<string, string>) -> string {
    match (x) {
        let t: map<string, T> if true => "map<T>",
        let s: map<string, string> => "map<string>",
    }
}

function pick_map_int_fn() -> string {
    let m: map<string, int> = { "k": 1 };
    pick_map<int>(m)
}

test "typevar_map_arm_matches_realized_value_type" {
    assert.equal(pick_map_int_fn(), "map<T>")
}

function pick_map_string_fn() -> string {
    let m: map<string, string> = { "k": "v" };
    pick_map<int>(m)
}

test "typevar_map_arm_rejects_other_value_type" {
    assert.equal(pick_map_string_fn(), "map<string>")
}

// ─── 4. `is T` ────────────────────────────────────────────────────────────────

function is_t<T>(x: T | string) -> bool {
    x is T
}

function is_t_member_fn() -> bool {
    is_t<int>(42)
}

test "is_typevar_member" {
    assert.is_true(is_t_member_fn())
}

function is_t_non_member_fn() -> bool {
    is_t<int>("hi")
}

test "is_typevar_non_member" {
    assert.is_true(!is_t_non_member_fn())
}
