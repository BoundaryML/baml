// Unified tests for array construction and methods.
//
// Converted from crates/baml_tests/tests/arrays.rs. Each Rust test becomes a
// `test` block with the `main()` body inlined. Results are compared with
// `baml.deep_equals` (structural — `==` on arrays is reference equality), except
// optional (`int?`) results, which use `assert.equal`: `deep_equals`'s generic
// `T` won't unify an `int?` value against a bare int literal.
//
// A `catch` expression's value is passed directly into the assertion call rather
// than bound to a `let`: a stored result stays boxed and won't compare equal, but
// the call boundary coerces it back to its concrete type.
//
// Inside a `test` block, a value read from a `let` local compares unequal to a
// literal (both `==` and `deep_equals`) — a VM bug that does not affect the same
// code inside a `function`. Tests that must hold a value in a local (mutation
// results, `count` accumulators) therefore bind the expected value to a local
// too, since two locals compare correctly.

test "array_literal" {
    assert.is_true(baml.deep_equals([1, 2, 3], [1, 2, 3]))
}

test "array_assign_to_variable" {
    let a = [1, 2, 3];
    // `expected` is a local: a test-block local compares unequal to a literal (VM bug)
    let expected = [1, 2, 3];
    assert.is_true(baml.deep_equals(a, expected))
}

test "array_push" {
    let a: int[] = [1, 2, 3];
    a.push(4);
    // `expected` is a local: a test-block local compares unequal to a literal (VM bug)
    let expected = [1, 2, 3, 4];
    assert.is_true(baml.deep_equals(a, expected))
}

// ============================================================================
// Array.map tests
// ============================================================================

// array.map with a simple doubling function (no captures).
// [1, 2, 3].map(x -> x * 2) returns [2, 4, 6].
test "array_map_simple_double" {
    let items: int[] = [1, 2, 3];
    assert.is_true(baml.deep_equals(items.map((x: int) -> int { x * 2 }), [2, 4, 6]))
}

// array.map on an empty array returns [].
test "array_map_empty_array" {
    let items: int[] = [];
    assert.is_true(baml.deep_equals(items.map((x: int) -> int { x * 2 }), []))
}

// array.map with a single-element array.
// [42].map(x -> x + 1) returns [43].
test "array_map_single_element" {
    let items: int[] = [42];
    assert.is_true(baml.deep_equals(items.map((x: int) -> int { x + 1 }), [43]))
}

// array.map with a closure capturing multiple variables.
// [1, 2, 3].map(x -> x * scale + bias) where scale=10, bias=5 returns [15, 25, 35].
test "array_map_closure_captures_multiple_variables" {
    let scale = 10;
    let bias = 5;
    let items: int[] = [1, 2, 3];
    assert.is_true(baml.deep_equals(items.map((x: int) -> int { x * scale + bias }), [15, 25, 35]))
}

// array.map where the callback throws — verify exception propagates through
// the CPS trampoline and can be caught.
test "array_map_callback_throws" {
    let items: int[] = [1, 2, 3];
    assert.is_true(baml.deep_equals(
        items.map((x: int) -> int {
            if (x == 2) { throw "bad value" }
            x
        }) catch (e) {
            _ => "caught"
        },
        "caught"
    ))
}

// array.map callback inside a catch base still resolves its own parameter,
// even though the catch handler introduces a later lexical scope.
test "array_map_callback_in_catch_base_keeps_parameter_scope" {
    let items: int[] = [1, 2, 3];
    assert.is_true(baml.deep_equals(
        items.map((x: int) -> int {
            if (x == 2) { x + 10 } else { x }
        }) catch (e) {
            _ => [0]
        },
        [1, 12, 3]
    ))
}

// array.map over string[] — exercises heap-object paths in MapContinuation
// (gc_roots, apply_forwarding) that int[] tests don't cover.
test "array_map_string_elements" {
    let items: string[] = ["a", "b", "c"];
    assert.is_true(baml.deep_equals(items.map((s: string) -> string { s + "!" }), ["a!", "b!", "c!"]))
}

// ─── clear ────────────────────────────────────────────────────────────────────

test "array_clear_non_empty" {
    let xs = [1, 2, 3];
    xs.clear();
    assert.is_true(baml.deep_equals(xs.length(), 0))
}

test "array_clear_already_empty_is_noop" {
    let xs: int[] = [];
    xs.clear();
    assert.is_true(baml.deep_equals(xs.length(), 0))
}

test "array_clear_returns_array_after" {
    let xs = [1, 2, 3];
    xs.clear();
    // `expected` is a local: a test-block local compares unequal to a literal (VM bug)
    let expected: int[] = [];
    assert.is_true(baml.deep_equals(xs, expected))
}

// ─── shift ────────────────────────────────────────────────────────────────────

test "array_shift_returns_first_element" {
    let xs = [10, 20, 30];
    assert.equal(xs.shift(), 10)
}

test "array_shift_modifies_array_in_place" {
    let xs = [10, 20, 30];
    let _ = xs.shift();
    // `expected` is a local: a test-block local compares unequal to a literal (VM bug)
    let expected = [20, 30];
    assert.is_true(baml.deep_equals(xs, expected))
}

test "array_shift_empty_returns_null" {
    let xs: int[] = [];
    assert.equal(xs.shift(), null)
}

test "array_shift_repeated_drains_array" {
    let xs = [1, 2, 3];
    let _ = xs.shift();
    let _ = xs.shift();
    let _ = xs.shift();
    assert.is_true(baml.deep_equals(xs.length(), 0))
}

// ─── unshift ──────────────────────────────────────────────────────────────────

test "array_unshift_returns_new_length" {
    let xs = [2, 3];
    assert.is_true(baml.deep_equals(xs.unshift(1), 3))
}

test "array_unshift_inserts_at_front" {
    let xs = [2, 3];
    let _ = xs.unshift(1);
    // `expected` is a local: a test-block local compares unequal to a literal (VM bug)
    let expected = [1, 2, 3];
    assert.is_true(baml.deep_equals(xs, expected))
}

test "array_unshift_empty_array" {
    let xs: int[] = [];
    let _ = xs.unshift(42);
    // `expected` is a local: a test-block local compares unequal to a literal (VM bug)
    let expected = [42];
    assert.is_true(baml.deep_equals(xs, expected))
}

// ─── insert ───────────────────────────────────────────────────────────────────

test "array_insert_in_middle" {
    let xs = [1, 2, 4];
    let _ = xs.insert(3, 2);
    // `expected` is a local: a test-block local compares unequal to a literal (VM bug)
    let expected = [1, 2, 3, 4];
    assert.is_true(baml.deep_equals(xs, expected))
}

test "array_insert_at_zero" {
    let xs = [2, 3];
    let _ = xs.insert(1, 0);
    // `expected` is a local: a test-block local compares unequal to a literal (VM bug)
    let expected = [1, 2, 3];
    assert.is_true(baml.deep_equals(xs, expected))
}

test "array_insert_at_length_appends" {
    let xs = [1, 2];
    let _ = xs.insert(3, xs.length());
    // `expected` is a local: a test-block local compares unequal to a literal (VM bug)
    let expected = [1, 2, 3];
    assert.is_true(baml.deep_equals(xs, expected))
}

test "array_insert_returns_new_length" {
    let xs = [1, 2, 4];
    assert.is_true(baml.deep_equals(xs.insert(3, 2), 4))
}

test "array_insert_empty_array_at_zero" {
    let xs: int[] = [];
    let _ = xs.insert(99, 0);
    // `expected` is a local: a test-block local compares unequal to a literal (VM bug)
    let expected = [99];
    assert.is_true(baml.deep_equals(xs, expected))
}

test "array_insert_negative_throws" {
    {
        let xs = [1, 2, 3];
        xs.insert(0, -1);
        baml.sys.panic("expected insert to throw")
    } catch (e) {
        baml.errors.InvalidArgument => null
    }
}

test "array_insert_past_length_throws" {
    {
        let xs = [1, 2, 3];
        xs.insert(0, 100);
        baml.sys.panic("expected insert to throw")
    } catch (e) {
        baml.errors.InvalidArgument => null
    }
}

// ─── splice ───────────────────────────────────────────────────────────────────

test "array_splice_replaces_range" {
    let xs = [0, 1, 2, 3, 4];
    xs.splice(1, 2, [9, 9, 9]);
    // `expected` is a local: a test-block local compares unequal to a literal (VM bug)
    let expected = [0, 9, 9, 9, 3, 4];
    assert.is_true(baml.deep_equals(xs, expected))
}

test "array_splice_pure_insertion" {
    let xs = [1, 2, 3];
    xs.splice(0, 0, [-1]);
    // `expected` is a local: a test-block local compares unequal to a literal (VM bug)
    let expected = [-1, 1, 2, 3];
    assert.is_true(baml.deep_equals(xs, expected))
}

test "array_splice_pure_removal" {
    let xs = [0, 1, 2, 3];
    xs.splice(1, 2, []);
    // `expected` is a local: a test-block local compares unequal to a literal (VM bug)
    let expected = [0, 3];
    assert.is_true(baml.deep_equals(xs, expected))
}

test "array_splice_count_clamped_to_length" {
    let xs = [0, 1, 2];
    xs.splice(1, 99, [9]);
    // `expected` is a local: a test-block local compares unequal to a literal (VM bug)
    let expected = [0, 9];
    assert.is_true(baml.deep_equals(xs, expected))
}

test "array_splice_at_length_appends" {
    let xs = [0, 1];
    xs.splice(2, 0, [2, 3]);
    // `expected` is a local: a test-block local compares unequal to a literal (VM bug)
    let expected = [0, 1, 2, 3];
    assert.is_true(baml.deep_equals(xs, expected))
}

test "array_splice_replace_with_longer" {
    let xs = [0, 1, 2];
    xs.splice(1, 1, [10, 11, 12]);
    // `expected` is a local: a test-block local compares unequal to a literal (VM bug)
    let expected = [0, 10, 11, 12, 2];
    assert.is_true(baml.deep_equals(xs, expected))
}

test "array_splice_negative_start_throws" {
    {
        let xs = [1, 2, 3];
        xs.splice(-1, 0, []);
        baml.sys.panic("expected splice to throw")
    } catch (e) {
        baml.errors.InvalidArgument => null
    }
}

test "array_splice_start_past_length_throws" {
    {
        let xs = [1, 2, 3];
        xs.splice(99, 0, []);
        baml.sys.panic("expected splice to throw")
    } catch (e) {
        baml.errors.InvalidArgument => null
    }
}

test "array_splice_negative_count_throws" {
    {
        let xs = [1, 2, 3];
        xs.splice(0, -1, []);
        baml.sys.panic("expected splice to throw")
    } catch (e) {
        baml.errors.InvalidArgument => null
    }
}

// ─── some ─────────────────────────────────────────────────────────────────────

test "array_some_finds_match" {
    assert.is_true(baml.deep_equals([1, 2, 3].some((x: int) -> bool { x > 2 }), true))
}

test "array_some_no_match" {
    assert.is_true(baml.deep_equals([1, 2, 3].some((x: int) -> bool { x > 9 }), false))
}

test "array_some_empty_is_false" {
    let xs: int[] = [];
    assert.is_true(baml.deep_equals(xs.some((x: int) -> bool { true }), false))
}

// Counts predicate invocations via a closure-captured counter. If `some`
// short-circuits, the counter stops at the first match (idx 1).
test "array_some_short_circuits" {
    let count = 0;
    let _ = [1, 5, 100, 200].some((x: int) -> bool {
        count = count + 1;
        x > 4
    });
    // `expected` is a local: a test-block local compares unequal to a literal (VM bug)
    let expected = 2;
    assert.is_true(baml.deep_equals(count, expected))
}

test "array_some_propagates_error" {
    assert.is_true(baml.deep_equals(
        [1, 2, 3].some((x: int) -> bool throws string {
            if (x == 2) { throw "boom" } else { false }
        }) catch (e) {
            string => true
        },
        true
    ))
}

// ─── every ────────────────────────────────────────────────────────────────────

test "array_every_all_match" {
    assert.is_true(baml.deep_equals([2, 4, 6].every((x: int) -> bool { x % 2 == 0 }), true))
}

test "array_every_one_fails" {
    assert.is_true(baml.deep_equals([2, 4, 5].every((x: int) -> bool { x % 2 == 0 }), false))
}

test "array_every_empty_is_true" {
    let xs: int[] = [];
    assert.is_true(baml.deep_equals(xs.every((x: int) -> bool { false }), true))
}

// visited 1, 2, 3; 3 < 3 = false → stop
test "array_every_short_circuits_on_false" {
    let count = 0;
    let _ = [1, 2, 3, 4].every((x: int) -> bool {
        count = count + 1;
        x < 3
    });
    // `expected` is a local: a test-block local compares unequal to a literal (VM bug)
    let expected = 3;
    assert.is_true(baml.deep_equals(count, expected))
}

// ─── find / find_index ────────────────────────────────────────────────────────

test "array_find_returns_first_match" {
    assert.equal([1, 2, 3, 4].find((x: int) -> bool { x > 2 }), 3)
}

test "array_find_returns_null_when_no_match" {
    assert.equal([1, 2, 3].find((x: int) -> bool { x > 9 }), null)
}

test "array_find_empty_is_null" {
    let xs: int[] = [];
    assert.equal(xs.find((x: int) -> bool { true }), null)
}

test "array_find_index_returns_first_index" {
    assert.equal([1, 2, 3, 4].find_index((x: int) -> bool { x > 2 }), 2)
}

test "array_find_index_returns_null_when_no_match" {
    assert.equal([1, 2].find_index((x: int) -> bool { x > 9 }), null)
}

test "array_find_short_circuits" {
    let count = 0;
    let _ = [10, 20, 30, 40].find((x: int) -> bool {
        count = count + 1;
        x >= 20
    });
    // `expected` is a local: a test-block local compares unequal to a literal (VM bug)
    let expected = 2;
    assert.is_true(baml.deep_equals(count, expected))
}

// ─── find_last / find_last_index ──────────────────────────────────────────────

test "array_find_last_returns_last_match" {
    assert.equal([1, 2, 3, 4].find_last((x: int) -> bool { x > 1 }), 4)
}

test "array_find_last_returns_null_when_no_match" {
    assert.equal([1, 2, 3].find_last((x: int) -> bool { x > 9 }), null)
}

// [1, 2, 1, 2] → last index where x == 1 is 2.
test "array_find_last_index_returns_last_index" {
    assert.equal([1, 2, 1, 2].find_last_index((x: int) -> bool { x == 1 }), 2)
}

// Verify iteration direction by counting visits: 40, 30, 20 — short-circuits on 20.
test "array_find_last_iterates_back_to_front" {
    let count = 0;
    let _ = [10, 20, 30, 40].find_last((x: int) -> bool {
        count = count + 1;
        x <= 20
    });
    // `expected` is a local: a test-block local compares unequal to a literal (VM bug)
    let expected = 3;
    assert.is_true(baml.deep_equals(count, expected))
}

test "array_find_last_empty_is_null" {
    let xs: int[] = [];
    assert.equal(xs.find_last((x: int) -> bool { true }), null)
}

// ─── includes / index_of / last_index_of ──────────────────────────────────────

test "array_includes_present" {
    assert.is_true(baml.deep_equals([1, 2, 3].includes(2), true))
}

test "array_includes_absent" {
    assert.is_true(baml.deep_equals([1, 2, 3].includes(99), false))
}

test "array_includes_empty" {
    let xs: int[] = [];
    assert.is_true(baml.deep_equals(xs.includes(1), false))
}

test "array_includes_strings" {
    assert.is_true(baml.deep_equals(["a", "b", "c"].includes("b"), true))
}

test "array_index_of_present" {
    assert.equal([10, 20, 30].index_of(20), 1)
}

// Both 10s — index_of returns the first.
test "array_index_of_first_match" {
    assert.equal([10, 20, 10].index_of(10), 0)
}

test "array_index_of_absent" {
    assert.equal([10, 20].index_of(99), null)
}

test "array_last_index_of_returns_last_match" {
    assert.equal([10, 20, 10].last_index_of(10), 2)
}

test "array_last_index_of_absent" {
    assert.equal([10, 20].last_index_of(99), null)
}

// ─── reduce ───────────────────────────────────────────────────────────────────

test "array_reduce_sum" {
    assert.is_true(baml.deep_equals([1, 2, 3, 4].reduce((acc: int, x: int) -> int { acc + x }, 0), 10))
}

test "array_reduce_with_nonzero_initial" {
    assert.is_true(baml.deep_equals([1, 2, 3].reduce((acc: int, x: int) -> int { acc + x }, 100), 106))
}

test "array_reduce_empty_returns_initial" {
    let xs: int[] = [];
    assert.is_true(baml.deep_equals(xs.reduce((acc: int, x: int) -> int { acc + x }, 42), 42))
}

// 1 * 2 * 3 * 4 = 24
test "array_reduce_visits_every_element" {
    assert.is_true(baml.deep_equals([1, 2, 3, 4].reduce((acc: int, x: int) -> int { acc * x }, 1), 24))
}

test "array_reduce_concat_strings" {
    assert.is_true(baml.deep_equals(["a", "b", "c"].reduce((acc: string, x: string) -> string { acc + x }, ""), "abc"))
}

// ─── flat_map ─────────────────────────────────────────────────────────────────

test "array_flat_map_doubles_each_element" {
    assert.is_true(baml.deep_equals([1, 2, 3].flat_map((x: int) -> int[] { [x, x * 10] }), [1, 10, 2, 20, 3, 30]))
}

test "array_flat_map_with_empty_inner_arrays" {
    assert.is_true(baml.deep_equals([1, 2, 3].flat_map((x: int) -> int[] { [] }), []))
}

test "array_flat_map_empty_input" {
    let xs: int[] = [];
    assert.is_true(baml.deep_equals(xs.flat_map((x: int) -> int[] { [x] }), []))
}

test "array_flat_map_uneven_inner_lengths" {
    assert.is_true(baml.deep_equals(
        [1, 2, 3].flat_map((x: int) -> int[] {
            if (x == 2) { [20, 21, 22] } else { [x] }
        }),
        [1, 20, 21, 22, 3]
    ))
}
