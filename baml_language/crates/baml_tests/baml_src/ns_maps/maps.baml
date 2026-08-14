// Unified tests for map construction, access, and methods.
//
// Converted from crates/baml_tests/tests/maps.rs.
//
// Map tests that mutate a local map and then read from it require a helper
// function because test-block locals are boxed (VM bug): reading a local
// after m.set(...) would return Object(Any) instead of the concrete int.

// ─── len ─────────────────────────────────────────────────────────────────────

function map_len_helper() -> int {
    let map = {
        "hi": 123,
        "it_works": 456
    };
    map.length()
}

test "len" {
    assert.equal(map_len_helper(), 2)
}

// ─── set returns displaced value ──────────────────────────────────────────────

function map_set_first_insert_main() -> int? {
    let m: map<string, int> = {};
    m.set("a", 1)
}

test "map_set_first_insert_returns_null" {
    assert.equal(map_set_first_insert_main(), null)
}

function map_set_replace_returns_previous_main() -> int? {
    let m: map<string, int> = {};
    let _ = m.set("a", 1);
    m.set("a", 2)
}

test "map_set_replace_returns_previous" {
    assert.equal(map_set_replace_returns_previous_main(), 1)
}

function map_set_then_get_main() -> int {
    let m: map<string, int> = {};
    let _ = m.set("a", 1);
    let _ = m.set("a", 2);
    m.get("a") ?? -1
}

test "map_set_then_get_reads_latest" {
    assert.equal(map_set_then_get_main(), 2)
}

function map_set_grows_length_main() -> int {
    let m: map<string, int> = {};
    let _ = m.set("a", 1);
    let _ = m.set("b", 2);
    let _ = m.set("c", 3);
    m.length()
}

test "map_set_grows_length" {
    assert.equal(map_set_grows_length_main(), 3)
}

function map_set_replace_preserves_length_main() -> int {
    let m: map<string, int> = {};
    let _ = m.set("a", 1);
    let _ = m.set("a", 2);
    let _ = m.set("a", 3);
    m.length()
}

test "map_set_replace_preserves_length" {
    assert.equal(map_set_replace_preserves_length_main(), 1)
}

// ─── delete ───────────────────────────────────────────────────────────────────

function map_delete_returns_previous_main() -> int? {
    let m: map<string, int> = {};
    let _ = m.set("a", 42);
    m.delete("a")
}

test "map_delete_returns_previous" {
    assert.equal(map_delete_returns_previous_main(), 42)
}

function map_delete_absent_main() -> int? {
    let m: map<string, int> = {};
    m.delete("missing")
}

test "map_delete_absent_returns_null" {
    assert.equal(map_delete_absent_main(), null)
}

function map_delete_already_deleted_main() -> int? {
    let m: map<string, int> = {};
    let _ = m.set("a", 1);
    let _ = m.delete("a");
    m.delete("a")
}

test "map_delete_already_deleted_returns_null" {
    assert.equal(map_delete_already_deleted_main(), null)
}

function map_delete_shrinks_length_main() -> int {
    let m: map<string, int> = {};
    let _ = m.set("a", 1);
    let _ = m.set("b", 2);
    let _ = m.delete("a");
    m.length()
}

test "map_delete_shrinks_length" {
    assert.equal(map_delete_shrinks_length_main(), 1)
}

function map_delete_does_not_affect_other_keys_main() -> int {
    let m: map<string, int> = {};
    let _ = m.set("a", 1);
    let _ = m.set("b", 2);
    let _ = m.delete("a");
    m.get("b") ?? -1
}

test "map_delete_does_not_affect_other_keys" {
    assert.equal(map_delete_does_not_affect_other_keys_main(), 2)
}

// ─── get_or_insert ────────────────────────────────────────────────────────────

function map_get_or_insert_inserts_when_absent_main() -> int {
    let m: map<string, int> = {};
    m.get_or_insert("k", 10)
}

test "map_get_or_insert_inserts_when_absent" {
    assert.equal(map_get_or_insert_inserts_when_absent_main(), 10)
}

function map_get_or_insert_returns_existing_main() -> int {
    let m: map<string, int> = {};
    let _ = m.set("k", 1);
    m.get_or_insert("k", 99)
}

test "map_get_or_insert_returns_existing_when_present" {
    assert.equal(map_get_or_insert_returns_existing_main(), 1)
}

function map_get_or_insert_does_not_overwrite_main() -> int {
    let m: map<string, int> = {};
    let _ = m.set("k", 1);
    let _ = m.get_or_insert("k", 99);
    m.get("k") ?? -1
}

test "map_get_or_insert_does_not_overwrite" {
    assert.equal(map_get_or_insert_does_not_overwrite_main(), 1)
}

function map_get_or_insert_persists_main() -> int {
    let m: map<string, int> = {};
    let _ = m.get_or_insert("k", 7);
    m.get("k") ?? -1
}

test "map_get_or_insert_persists_inserted_value" {
    assert.equal(map_get_or_insert_persists_main(), 7)
}

function map_get_or_insert_grows_length_main() -> int {
    let m: map<string, int> = {};
    let _ = m.get_or_insert("a", 1);
    let _ = m.get_or_insert("a", 2);
    m.length()
}

test "map_get_or_insert_grows_length_only_when_absent" {
    assert.equal(map_get_or_insert_grows_length_main(), 1)
}

// ─── clear ────────────────────────────────────────────────────────────────────

function map_clear_removes_all_main() -> int {
    let m: map<string, int> = {};
    let _ = m.set("a", 1);
    let _ = m.set("b", 2);
    m.clear();
    m.length()
}

test "map_clear_removes_all" {
    assert.equal(map_clear_removes_all_main(), 0)
}

function map_clear_already_empty_main() -> int {
    let m: map<string, int> = {};
    m.clear();
    m.length()
}

test "map_clear_already_empty_is_noop" {
    assert.equal(map_clear_already_empty_main(), 0)
}

function map_clear_then_reuse_main() -> int {
    let m: map<string, int> = {};
    let _ = m.set("a", 1);
    m.clear();
    let _ = m.set("b", 99);
    m.get("b") ?? -1
}

test "map_clear_then_reuse" {
    assert.equal(map_clear_then_reuse_main(), 99)
}

function map_clear_makes_get_return_null_main() -> int? {
    let m: map<string, int> = {};
    let _ = m.set("a", 1);
    m.clear();
    m.get("a")
}

test "map_clear_makes_get_return_null" {
    assert.equal(map_clear_makes_get_return_null_main(), null)
}

// ─── has integrates with new methods ──────────────────────────────────────────

function map_has_after_delete_main() -> bool {
    let m: map<string, int> = {};
    let _ = m.set("a", 1);
    let _ = m.delete("a");
    m.has("a")
}

test "map_has_after_delete_is_false" {
    assert.is_true(!map_has_after_delete_main())
}

function map_has_after_get_or_insert_main() -> bool {
    let m: map<string, int> = {};
    let _ = m.get_or_insert("k", 7);
    m.has("k")
}

test "map_has_after_get_or_insert_is_true" {
    assert.is_true(map_has_after_get_or_insert_main())
}

// ─── map element subscript access (m[key]) ────────────────────────────────────

function create_map_helper() -> map<string, string> {
    { "hello": "world" }
}

function use_map_helper() -> string {
    let map = create_map_helper();
    map["hello"]
}

test "create_and_access" {
    assert.equal(use_map_helper(), "world")
}

// access_no_key: accessing a missing key should throw MapKeyNotFound.

function use_map_no_key_helper() -> string {
    let map = create_map_helper();
    map["world"]
}

test "access_no_key" {
    {
        use_map_no_key_helper();
        baml.sys.panic("expected map access to throw")
    } catch (e) {
        baml.panics.MapKeyNotFound => null
    }
}

// contains: use map.has() before subscript access.

function use_map_contains_helper() -> string {
    let map = { "hello": "world" };
    if (map.has("hello")) {
        map["hello"]
    } else {
        "hi"
    }
}

test "contains" {
    assert.equal(use_map_contains_helper(), "world")
}

// modify: assign and compound-assign via subscript, then read back.

function edit_map_key_helper() -> int {
    let map = { "hi": 123 };
    map["hi"] = 42 - 4;
    map["hi"] += 4;
    map["hi"]
}

test "modify" {
    assert.equal(edit_map_key_helper(), 42)
}
