// Runtime coverage for the `baml.Array.filled(n, value)` stdlib constructor
// (`baml/containers.baml`). Confirms it allocates a runtime-sized array
// pre-initialized to `value`, widens the element type from `value`, and clamps
// non-positive `n` to an empty array.
//
// Converted from crates/bex_engine/tests/array_filled.rs. Each Rust test's
// `function main()` becomes a top-level helper named after the test — the VM
// boxes `test`-block locals (a local compares unequal to a literal), so every
// value under test is produced by a function call and asserted on directly.

// ─── array_filled_length_and_value ────────────────────────────────────────────

// `baml.Array.filled(n, value)` returns an array of length `n` with every slot
// set to `value` (the canonical zero-initialized buffer use case).
function array_filled_length_and_value() -> int {
    let xs = baml.Array.filled(5, 0);
    let total = xs.length();
    for (let x in xs) {
        if (x != 0) { total += 100; }
    }
    total
}

test "array_filled_length_and_value" {
    assert.equal(array_filled_length_and_value(), 5)
}

// ─── array_filled_non_positive_is_empty ───────────────────────────────────────

// A non-positive `n` clamps to an empty array rather than panicking.
function array_filled_non_positive_is_empty() -> int {
    baml.Array.filled(0, 7).length() + baml.Array.filled(-3, 7).length()
}

test "array_filled_non_positive_is_empty" {
    assert.equal(array_filled_non_positive_is_empty(), 0)
}

// ─── array_filled_infers_element_type ─────────────────────────────────────────

// The element type is inferred from `value`, so a string fill yields a
// `string[]` whose entries are usable as strings.
function array_filled_infers_element_type() -> string {
    let xs = baml.Array.filled(3, "ab");
    xs.join("-")
}

test "array_filled_infers_element_type" {
    assert.equal(array_filled_infers_element_type(), "ab-ab-ab")
}

// ─── array_filled_mutable_value_aliases_all_slots ─────────────────────────────

// Filling with a mutable reference type aliases that same object in each slot:
// mutating one inner array is visible through all of them. If each slot had an
// independent copy we'd get 4 (2 + 1 + 1); the current `Array.filled` contract
// aliases the same inner array, so all rows have length 2 after one push:
// 2 + 2 + 2 = 6.
function array_filled_mutable_value_aliases_all_slots() -> int {
    let rows = baml.Array.filled(3, [0]);
    match (rows.at(0)) {
        null => 0,
        let first: int[] => {
            first.push(1);
            let total = 0;
            for (let row in rows) {
                total += row.length();
            }
            total
        }
    }
}

test "array_filled_mutable_value_aliases_all_slots" {
    assert.equal(array_filled_mutable_value_aliases_all_slots(), 6)
}
